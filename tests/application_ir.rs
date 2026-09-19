use fun_refactor::application_ir::{
    write_routes, Adapter, FeatureKind, HttpDependency, HttpExpression, HttpInput, HttpInputSource,
    HttpMiddleware, HttpRoute, HttpScalar, RouteBundle,
};
use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn floating_merkle_addresses_match_python_over_boundary_and_bit_cases() {
    let mut values = vec![
        1e-7_f64,
        1e-6,
        1e-5,
        1e-4,
        1e15,
        1e16,
        1e20,
        -0.0,
        1.25e-5,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::from_bits(1),
    ];
    let mut bits = 0x1234_5678_90ab_cdef_u64;
    for _ in 0..4096 {
        bits = bits
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = f64::from_bits(bits);
        if value.is_finite() {
            values.push(value);
        }
    }
    let cases = values
        .iter()
        .map(|number| {
            let value = json!(number);
            json!({"digest": fun_refactor::project::object_merkle(&value).unwrap(), "value": value})
        })
        .collect::<Vec<_>>();
    let mut child = Command::new("python3")
        .env(
            "PYTHONPATH",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sdk/python/src"),
        )
        .args(["-c", include_str!("application-runtime/numbers.py")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        cases.len().to_string()
    );
}

fn route(path: &str) -> HttpRoute {
    HttpRoute {
        method: "GET".into(),
        path: path.into(),
        inputs: Vec::new(),
        dependencies: Vec::new(),
        status: 200,
        response: HttpExpression::Literal { value: json!(true) },
    }
}

#[test]
fn portable_admission_refuses_dispatch_and_number_ambiguities() {
    for path in [
        "relative", "/a/", "/a//b", "/..", "/%61", "/a?b", "/a#b", "/é", "/{x}/{x}", "/{9x}",
        "/a:b", "/x*", "/[x]",
    ] {
        assert!(route(path).validate().is_err(), "{path}");
    }
    for value in [
        json!(9007199254740992_i64),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut route = route("/");
        route.response = HttpExpression::Literal { value };
        assert!(route.validate().is_err());
    }
    for status in [0, 199, 204, 205, 304, 600] {
        let mut route = route("/");
        route.status = status;
        assert!(route.validate().is_err());
    }
    for (left, right) in [
        ("/x", "/x"),
        ("/x", "/{x}"),
        ("/{x}", "/{y}"),
        ("/{x}/a", "/b/{y}"),
    ] {
        assert!(write_routes(&[route(left), route(right)], &[], Adapter::Nextjs).is_err());
    }
    let mut post = route("/{y}");
    post.method = "POST".into();
    assert!(write_routes(&[route("/{x}"), post], &[], Adapter::Nextjs).is_err());
    assert!(write_routes(&[route("/x")], &[], Adapter::React).is_err());
}

#[test]
fn every_http_writer_preserves_nested_values_and_parses() {
    let response = HttpExpression::Object {
        fields: BTreeMap::from([
            (
                "__proto__".into(),
                HttpExpression::Object {
                    fields: BTreeMap::from([(
                        "null".into(),
                        HttpExpression::Literal { value: json!(null) },
                    )]),
                },
            ),
            (
                "value".into(),
                HttpExpression::Path {
                    name: "JSONResponse".into(),
                },
            ),
            (
                "array".into(),
                HttpExpression::Array {
                    items: vec![
                        HttpExpression::Literal {
                            value: json!("é\n\"\\"),
                        },
                        HttpExpression::Literal {
                            value: json!(-9007199254740991_i64),
                        },
                    ],
                },
            ),
        ]),
    };
    let route = HttpRoute {
        response,
        path: "/records/{JSONResponse}".into(),
        ..route("/")
    };
    route.validate().unwrap();
    let value = route
        .response
        .evaluate(&BTreeMap::from([("JSONResponse".into(), "chosen".into())]))
        .unwrap();
    assert_eq!(value["value"], "chosen");
    assert_eq!(value["__proto__"]["null"], json!(null));
    for (adapter, language) in [
        (Adapter::Nextjs, Language::TypeScript),
        (Adapter::Express, Language::TypeScript),
        (Adapter::Fastapi, Language::Python),
        (Adapter::GoNetHttp, Language::Go),
    ] {
        for (path, source) in write_routes(std::slice::from_ref(&route), &[], adapter).unwrap() {
            let parsed = Parsers::new().parse(language, &source).unwrap();
            assert!(!parsed.has_errors(), "{adapter:?}: {path}: {source}");
        }
    }
    assert!(route.response.evaluate(&BTreeMap::new()).is_err());
}

#[test]
fn bundles_and_recursive_expressions_are_bounded_and_strict() {
    assert!(serde_json::from_value::<RouteBundle>(
        json!({"schema": "fr-http-application-1", "routes": [], "runtime_proved": true})
    )
    .is_err());
    let mut expr = HttpExpression::Literal {
        value: json!(false),
    };
    for _ in 0..34 {
        expr = HttpExpression::Array { items: vec![expr] };
    }
    assert!(expr.validate(&Default::default()).is_err());
    let expr = HttpExpression::Array {
        items: vec![HttpExpression::Literal { value: json!(null) }; 1024],
    };
    assert!(expr.validate(&Default::default()).is_err());
    let routes = vec![route("/"); 257];
    assert!(write_routes(&routes, &[], Adapter::Fastapi).is_err());
}

#[test]
fn validated_request_inputs_are_typed_bounded_and_deterministic() {
    let route = HttpRoute {
        method: "POST".into(),
        path: "/records/{id}".into(),
        dependencies: Vec::new(),
        inputs: vec![
            HttpInput {
                name: "limit".into(),
                source: HttpInputSource::Query,
                scalar: HttpScalar::Integer,
            },
            HttpInput {
                name: "active".into(),
                source: HttpInputSource::Query,
                scalar: HttpScalar::Boolean,
            },
            HttpInput {
                name: "title".into(),
                source: HttpInputSource::JsonBody,
                scalar: HttpScalar::String,
            },
        ],
        status: 201,
        response: HttpExpression::Object {
            fields: BTreeMap::from([
                ("id".into(), HttpExpression::Path { name: "id".into() }),
                (
                    "limit".into(),
                    HttpExpression::Input {
                        name: "limit".into(),
                    },
                ),
                (
                    "active".into(),
                    HttpExpression::Input {
                        name: "active".into(),
                    },
                ),
                (
                    "title".into(),
                    HttpExpression::Input {
                        name: "title".into(),
                    },
                ),
            ]),
        },
    };
    route.validate().unwrap();
    let source = write_routes(std::slice::from_ref(&route), &[], Adapter::Fastapi)
        .unwrap()
        .remove("routes.py")
        .unwrap();
    let mut python = Command::new("python3")
        .args([
            "-c",
            "import sys; compile(sys.stdin.read(), '<generated-fastapi>', 'exec')",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    python
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = python.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        route.feature_kind().unwrap(),
        FeatureKind::ValidatedJsonRoute
    );
    let values = route
        .validate_request(
            &BTreeMap::from([
                ("limit".into(), "12".into()),
                ("active".into(), "false".into()),
            ]),
            Some(&json!({"title": "chosen"})),
        )
        .unwrap();
    assert_eq!(values["limit"], 12);
    assert_eq!(values["active"], false);
    assert_eq!(values["title"], "chosen");
    assert_eq!(
        route
            .response
            .evaluate_with_inputs(&BTreeMap::from([("id".into(), "abc".into())]), &values)
            .unwrap(),
        json!({"active":false,"id":"abc","limit":12,"title":"chosen"})
    );

    for invalid in ["01", "+1", "1.0", "9007199254740992"] {
        let issues = route
            .validate_request(
                &BTreeMap::from([
                    ("limit".into(), invalid.into()),
                    ("active".into(), "false".into()),
                ]),
                Some(&json!({"title":"chosen"})),
            )
            .unwrap_err();
        assert_eq!(issues.len(), 1, "{invalid}");
        assert_eq!(issues[0].name, "limit");
    }
    let issues = route
        .validate_request(&BTreeMap::new(), Some(&json!({"title": 1})))
        .unwrap_err();
    assert_eq!(
        issues
            .iter()
            .map(|issue| issue.name.as_str())
            .collect::<Vec<_>>(),
        ["limit", "active", "title"]
    );
}

#[test]
fn service_calls_resolve_exactly_one_bundle_target() {
    let upstream = HttpRoute {
        method: "GET".into(),
        path: "/records".into(),
        inputs: Vec::new(),
        dependencies: Vec::new(),
        status: 200,
        response: HttpExpression::Object {
            fields: BTreeMap::from([(
                "items".into(),
                HttpExpression::Array {
                    items: vec![HttpExpression::Literal { value: json!("a") }],
                },
            )]),
        },
    };
    let mut feed = route("/feed");
    feed.response = HttpExpression::Service {
        method: "GET".into(),
        path: "/records".into(),
    };
    feed.validate().unwrap();
    let routes = [upstream.clone(), feed.clone()];
    for (adapter, language) in [
        (Adapter::Nextjs, Language::TypeScript),
        (Adapter::Express, Language::TypeScript),
        (Adapter::Fastapi, Language::Python),
        (Adapter::GoNetHttp, Language::Go),
    ] {
        for (path, source) in write_routes(&routes, &[], adapter).unwrap() {
            let parsed = Parsers::new().parse(language, &source).unwrap();
            assert!(!parsed.has_errors(), "{adapter:?}: {path}: {source}");
        }
    }
    assert!(feed.response.evaluate(&BTreeMap::new()).is_err());

    for bad in [
        "",
        "records",
        "/records/{id}",
        "/records?all=1",
        "https://x/records",
        "/a//b",
    ] {
        let mut invalid = route("/feed");
        invalid.response = HttpExpression::Service {
            method: "GET".into(),
            path: bad.into(),
        };
        assert!(invalid.validate().is_err(), "{bad}");
    }
    let mut wrong_method = route("/feed");
    wrong_method.response = HttpExpression::Service {
        method: "POST".into(),
        path: "/records".into(),
    };
    assert!(write_routes(&[upstream.clone(), wrong_method], &[], Adapter::Express).is_err());
    assert!(write_routes(std::slice::from_ref(&feed), &[], Adapter::Express).is_err());
    let mut selfish = route("/loop");
    selfish.response = HttpExpression::Service {
        method: "GET".into(),
        path: "/loop".into(),
    };
    assert!(write_routes(std::slice::from_ref(&selfish), &[], Adapter::Express).is_err());
    let mut unresolved = feed.clone();
    unresolved.response = HttpExpression::Service {
        method: "GET".into(),
        path: "/absent".into(),
    };
    assert!(write_routes(&[upstream, unresolved], &[], Adapter::Express).is_err());
}

#[test]
fn component_state_and_events_require_declared_client_boundaries() {
    use fun_refactor::application_ir::{
        write_static_component, ComponentEvent, ComponentState, StaticComponent, StaticNode,
    };
    let state = ComponentState {
        name: "open".into(),
        setter: "setOpen".into(),
        initial: json!(false),
    };
    let component = StaticComponent {
        name: "Panel".into(),
        client: true,
        state: vec![state.clone()],
        root: StaticNode::Element {
            tag: "button".into(),
            attributes: BTreeMap::new(),
            events: BTreeMap::from([(
                "onClick".into(),
                ComponentEvent::ToggleState {
                    state: "open".into(),
                },
            )]),
            children: vec![StaticNode::State {
                name: "open".into(),
            }],
        },
    };
    component.validate().unwrap();
    for adapter in [Adapter::React, Adapter::Nextjs] {
        let (path, source) = write_static_component(&component, adapter).unwrap();
        let parsed = Parsers::new().parse(Language::Tsx, &source).unwrap();
        assert!(!parsed.has_errors(), "{adapter:?} {path}: {source}");
        assert!(source.contains("useState(false)"), "{source}");
    }
    let mut server = component.clone();
    server.client = false;
    assert!(server.validate().is_err());
    let mut unknown = component.clone();
    unknown.root = StaticNode::State {
        name: "missing".into(),
    };
    assert!(unknown.validate().is_err());
    let mut mistyped = component.clone();
    mistyped.root = StaticNode::Element {
        tag: "button".into(),
        attributes: BTreeMap::new(),
        events: BTreeMap::from([(
            "onClick".into(),
            ComponentEvent::SetState {
                state: "open".into(),
                value: json!("yes"),
            },
        )]),
        children: vec![],
    };
    assert!(mistyped.validate().is_err());
    let mut float = component.clone();
    float.state = vec![ComponentState {
        name: "ratio".into(),
        setter: "setRatio".into(),
        initial: json!(0.5),
    }];
    assert!(float.validate().is_err());
    let mut renamed = component.clone();
    renamed.state = vec![
        state.clone(),
        ComponentState {
            name: "open".into(),
            setter: "setAgain".into(),
            initial: json!(true),
        },
    ];
    assert!(renamed.validate().is_err());
    let mut bad_event = component.clone();
    bad_event.root = StaticNode::Element {
        tag: "button".into(),
        attributes: BTreeMap::new(),
        events: BTreeMap::from([(
            "onclick".into(),
            ComponentEvent::ToggleState {
                state: "open".into(),
            },
        )]),
        children: vec![],
    };
    assert!(bad_event.validate().is_err());
    let serialized = serde_json::to_value(&component).unwrap();
    let round: StaticComponent = serde_json::from_value(serialized).unwrap();
    assert_eq!(round, component);
}

#[test]
fn validated_request_inputs_refuse_ambiguous_or_unportable_shapes() {
    let mut candidate = route("/records/{id}");
    candidate.inputs.push(HttpInput {
        name: "id".into(),
        source: HttpInputSource::Query,
        scalar: HttpScalar::String,
    });
    assert!(candidate.validate().is_err());
    candidate.inputs[0].name = "query".into();
    candidate.response = HttpExpression::Input {
        name: "missing".into(),
    };
    assert!(candidate.validate().is_err());
    candidate.response = HttpExpression::Input {
        name: "query".into(),
    };
    candidate.inputs.push(candidate.inputs[0].clone());
    assert!(candidate.validate().is_err());
    candidate.inputs.pop();
    candidate.inputs[0].source = HttpInputSource::JsonBody;
    assert!(candidate.validate().is_err());
}

fn middleware(name: &str, request_order: usize) -> HttpMiddleware {
    HttpMiddleware {
        name: name.into(),
        request_order,
    }
}

#[test]
fn middleware_chains_are_ordered_bounded_and_strict() {
    let routes = [route("/x")];
    let chain = [middleware("alpha", 1), middleware("beta", 2)];
    for (adapter, language, file) in [
        (Adapter::Nextjs, Language::TypeScript, "middleware.ts"),
        (Adapter::Express, Language::TypeScript, "routes.ts"),
        (Adapter::GoNetHttp, Language::Go, "routes.go"),
    ] {
        let files = write_routes(&routes, &chain, adapter).unwrap();
        let parsed = Parsers::new().parse(language, &files[file]).unwrap();
        assert!(!parsed.has_errors(), "{adapter:?}: {}", files[file]);
    }
    let dotted = [middleware("alpha", 1), middleware("ops.beta", 2)];
    let next = write_routes(&routes, &dotted, Adapter::Nextjs).unwrap();
    let source = &next["middleware.ts"];
    let alpha = source.find("frMiddleware.alpha").unwrap();
    let beta = source.find("frMiddleware.ops.beta").unwrap();
    assert!(alpha < beta, "{source}");
    let go = write_routes(&routes, &chain, Adapter::GoNetHttp).unwrap();
    let source = &go["routes.go"];
    let beta = source.find("handler = beta(handler)").unwrap();
    let alpha = source.find("handler = alpha(handler)").unwrap();
    assert!(beta < alpha, "{source}");
    assert!(write_routes(&routes, &chain, Adapter::Fastapi).is_err());
    assert!(write_routes(&routes, &chain, Adapter::React).is_err());
    assert!(write_routes(&routes, &[middleware("ops.beta", 1)], Adapter::GoNetHttp).is_err());
    for chain in [
        vec![middleware("alpha", 0)],
        vec![middleware("alpha", 1), middleware("beta", 1)],
        vec![middleware("alpha", 2)],
        vec![middleware("", 1)],
        vec![middleware("a..b", 1)],
        vec![middleware("a b", 1)],
    ] {
        assert!(
            write_routes(&routes, &chain, Adapter::Express).is_err(),
            "{chain:?}"
        );
    }
    let oversized: Vec<_> = (1..=65).map(|index| middleware("alpha", index)).collect();
    assert!(write_routes(&routes, &oversized, Adapter::Express).is_err());
    let bundle: RouteBundle = serde_json::from_value(json!({
        "schema": "fr-http-application-1",
        "routes": [route("/x")],
        "middleware": [{"name": "alpha", "request_order": 1}],
    }))
    .unwrap();
    bundle.validate().unwrap();
}

#[test]
fn route_dependencies_are_ordered_bounded_and_strict() {
    let mut candidate = route("/records");
    candidate.dependencies.push(HttpDependency {
        binding: "session".into(),
        provider: "auth.session".into(),
        security: true,
    });
    candidate.validate().unwrap();
    for adapter in [Adapter::Fastapi, Adapter::Express, Adapter::Nextjs] {
        assert!(write_routes(std::slice::from_ref(&candidate), &[], adapter).is_ok());
    }
    let mut duplicate = candidate.clone();
    duplicate.dependencies.push(HttpDependency {
        binding: "session".into(),
        provider: "other".into(),
        security: false,
    });
    assert!(duplicate.validate().is_err());
    let mut colliding = candidate.clone();
    colliding.dependencies[0].binding = "records".into();
    colliding.path = "/records/{records}".into();
    assert!(colliding.validate().is_err());
    let mut dotted = candidate.clone();
    dotted.dependencies[0].provider = "auth..session".into();
    assert!(dotted.validate().is_err());
    let mut oversized = candidate.clone();
    oversized.dependencies = (0..17)
        .map(|index| HttpDependency {
            binding: format!("dependency{index}"),
            provider: "provider".into(),
            security: false,
        })
        .collect();
    assert!(oversized.validate().is_err());
    let source = write_routes(std::slice::from_ref(&candidate), &[], Adapter::Fastapi)
        .unwrap()
        .remove("routes.py")
        .unwrap();
    assert!(
        source.contains("session=Security(fr_dependencies.auth.session)"),
        "{source}"
    );
    let mut python = Command::new("python3")
        .args([
            "-c",
            "import sys; compile(sys.stdin.read(), '<generated-fastapi>', 'exec')",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    python
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = python.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let express = write_routes(std::slice::from_ref(&candidate), &[], Adapter::Express)
        .unwrap()
        .remove("routes.ts")
        .unwrap();
    assert!(
        express.contains("await frDependencies.auth.session(req);") && express.contains("401"),
        "{express}"
    );
    let parsed = Parsers::new()
        .parse(Language::TypeScript, &express)
        .unwrap();
    assert!(!parsed.has_errors(), "{express}");
    let mut go_dotted = candidate.clone();
    assert!(write_routes(std::slice::from_ref(&go_dotted), &[], Adapter::GoNetHttp).is_err());
    go_dotted.dependencies[0].provider = "session".into();
    let go = write_routes(std::slice::from_ref(&go_dotted), &[], Adapter::GoNetHttp)
        .unwrap()
        .remove("routes.go")
        .unwrap();
    assert!(go.contains("if err := session(r); err != nil"), "{go}");
    let parsed = Parsers::new().parse(Language::Go, &go).unwrap();
    assert!(!parsed.has_errors(), "{go}");
}
