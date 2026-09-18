use fun_refactor::application_ir::{
    write_routes, Adapter, FeatureKind, HttpExpression, HttpInput, HttpInputSource, HttpRoute,
    HttpScalar, RouteBundle,
};
use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn floating_merkle_addresses_match_python_over_boundary_and_bit_cases() {
    use std::io::Write;
    use std::process::{Command, Stdio};
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
        assert!(write_routes(&[route(left), route(right)], Adapter::Nextjs).is_err());
    }
    let mut post = route("/{y}");
    post.method = "POST".into();
    assert!(write_routes(&[route("/{x}"), post], Adapter::Nextjs).is_err());
    assert!(write_routes(&[route("/x")], Adapter::React).is_err());
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
        for (path, source) in write_routes(std::slice::from_ref(&route), adapter).unwrap() {
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
    assert!(write_routes(&routes, Adapter::Fastapi).is_err());
}

#[test]
fn validated_request_inputs_are_typed_bounded_and_deterministic() {
    let route = HttpRoute {
        method: "POST".into(),
        path: "/records/{id}".into(),
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
