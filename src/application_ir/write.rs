use super::{
    Adapter, HttpExpression, HttpInputSource, HttpRoute, HttpScalar, StaticComponent, StaticNode,
};
use std::collections::BTreeMap;

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}

fn static_node(node: &StaticNode) -> String {
    match node {
        StaticNode::Text { value } => value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
        StaticNode::Element {
            tag,
            attributes,
            children,
        } => {
            let attributes = attributes
                .iter()
                .map(|(name, value)| {
                    let value = value
                        .replace('&', "&amp;")
                        .replace('"', "&quot;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;");
                    format!(" {name}=\"{value}\"")
                })
                .collect::<String>();
            if children.is_empty() {
                format!("<{tag}{attributes} />")
            } else {
                let children = children.iter().map(static_node).collect::<String>();
                format!("<{tag}{attributes}>{children}</{tag}>")
            }
        }
    }
}

pub fn write_static_component(
    component: &StaticComponent,
    adapter: Adapter,
) -> Result<(String, String), String> {
    component.validate()?;
    let (path, name) = match adapter {
        Adapter::React => ("App.tsx", "App"),
        Adapter::Nextjs => ("page.tsx", "Page"),
        _ => return Err("target adapter does not write static frontend components.".into()),
    };
    Ok((
        path.into(),
        format!(
            "export default function {name}() {{\n  return ({});\n}}\n",
            static_node(&component.root)
        ),
    ))
}

fn expression(
    expr: &HttpExpression,
    adapter: Adapter,
    bindings: &BTreeMap<String, String>,
) -> String {
    match expr {
        HttpExpression::Literal { value } => match (adapter, value) {
            (Adapter::Fastapi, serde_json::Value::Null) => "None".into(),
            (Adapter::Fastapi, serde_json::Value::Bool(true)) => "True".into(),
            (Adapter::Fastapi, serde_json::Value::Bool(false)) => "False".into(),
            (Adapter::GoNetHttp, serde_json::Value::Null) => "nil".into(),
            _ => value.to_string(),
        },
        HttpExpression::Path { name } => bindings[name].clone(),
        HttpExpression::Input { name } => bindings[name].clone(),
        HttpExpression::Object { fields } => {
            let fields = fields
                .iter()
                .map(|(name, value)| {
                    let key = if matches!(adapter, Adapter::Express | Adapter::Nextjs) {
                        format!("[{}]", quoted(name))
                    } else {
                        quoted(name)
                    };
                    format!("{key}: {}", expression(value, adapter, bindings))
                })
                .collect::<Vec<_>>()
                .join(", ");
            if adapter == Adapter::GoNetHttp {
                format!("map[string]any{{{fields}}}")
            } else {
                format!("{{{fields}}}")
            }
        }
        HttpExpression::Array { items } => {
            let items = items
                .iter()
                .map(|value| expression(value, adapter, bindings))
                .collect::<Vec<_>>()
                .join(", ");
            if adapter == Adapter::GoNetHttp {
                format!("[]any{{{items}}}")
            } else {
                format!("[{items}]")
            }
        }
    }
}

fn source(input: HttpInputSource) -> &'static str {
    match input {
        HttpInputSource::Query => "query",
        HttpInputSource::JsonBody => "json-body",
    }
}

fn scalar(input: HttpScalar) -> &'static str {
    match input {
        HttpScalar::String => "string",
        HttpScalar::Integer => "integer",
        HttpScalar::Boolean => "boolean",
    }
}

const TYPESCRIPT_VALIDATION: &str = r#"type FrInputKind = "string" | "integer" | "boolean";
type FrInputSource = "query" | "json-body";
type FrIssue = { source: FrInputSource; name: string; expected: FrInputKind };

function frReadInput(value: unknown, source: FrInputSource, name: string, expected: FrInputKind, issues: FrIssue[]): unknown {
  let parsed: unknown;
  if (source === "query") {
    if (typeof value !== "string") { issues.push({ source, name, expected }); return null; }
    if (expected === "string" && new TextEncoder().encode(value).length <= 4096) parsed = value;
    if (expected === "integer" && /^-?(0|[1-9][0-9]*)$/.test(value)) {
      const number = Number(value);
      if (Number.isSafeInteger(number) && String(number) === value) parsed = number;
    }
    if (expected === "boolean" && (value === "true" || value === "false")) parsed = value === "true";
  } else {
    if (expected === "string" && typeof value === "string" && new TextEncoder().encode(value).length <= 4096) parsed = value;
    if (expected === "integer" && typeof value === "number" && Number.isSafeInteger(value)) parsed = value;
    if (expected === "boolean" && typeof value === "boolean") parsed = value;
  }
  if (parsed === undefined) { issues.push({ source, name, expected }); return null; }
  return parsed;
}

"#;

const PYTHON_VALIDATION: &str = r#"def _fr_read_input(value, source, name, expected, issues):
    parsed = None
    valid = False
    if source == "query" and isinstance(value, str):
        if expected == "string" and len(value.encode("utf-8")) <= 4096:
            parsed, valid = value, True
        elif expected == "integer":
            try:
                candidate = int(value)
                if str(candidate) == value and -(2**53 - 1) <= candidate <= 2**53 - 1:
                    parsed, valid = candidate, True
            except ValueError:
                pass
        elif expected == "boolean" and value in ("true", "false"):
            parsed, valid = value == "true", True
    elif source == "json-body":
        if expected == "string" and isinstance(value, str) and len(value.encode("utf-8")) <= 4096:
            parsed, valid = value, True
        elif expected == "integer" and type(value) is int and -(2**53 - 1) <= value <= 2**53 - 1:
            parsed, valid = int(value), True
        elif expected == "integer" and type(value) is float and math.isfinite(value) and value.is_integer() and -(2**53 - 1) <= value <= 2**53 - 1:
            parsed, valid = int(value), True
        elif expected == "boolean" and type(value) is bool:
            parsed, valid = value, True
    if not valid:
        issues.append({"source": source, "name": name, "expected": expected})
    return parsed

def _fr_query_input(request, name):
    values = request.query_params.getlist(name)
    return values[0] if len(values) == 1 else None

"#;

const GO_VALIDATION: &str = r#"func frReadInput(value any, present bool, source, name, expected string) (any, map[string]any) {
	if present && source == "query" {
		text, ok := value.(string)
		if ok {
			switch expected {
			case "string":
				if len(text) <= 4096 { return text, nil }
			case "integer":
				if number, err := strconv.ParseInt(text, 10, 64); err == nil && strconv.FormatInt(number, 10) == text && number >= -9007199254740991 && number <= 9007199254740991 { return number, nil }
			case "boolean":
				if text == "true" { return true, nil }
				if text == "false" { return false, nil }
			}
		}
	}
	if present && source == "json-body" {
		switch expected {
		case "string":
			if text, ok := value.(string); ok && len(text) <= 4096 { return text, nil }
		case "integer":
			if text, ok := value.(json.Number); ok {
				if number, err := strconv.ParseFloat(string(text), 64); err == nil && number >= -9007199254740991 && number <= 9007199254740991 && number == float64(int64(number)) { return int64(number), nil }
			}
		case "boolean":
			if flag, ok := value.(bool); ok { return flag, nil }
		}
	}
	return nil, map[string]any{"source": source, "name": name, "expected": expected}
}

func frQueryInput(values []string) (any, bool) {
	if len(values) != 1 { return nil, false }
	return values[0], true
}

"#;

fn python_request(route: &HttpRoute) -> String {
    if route.inputs.is_empty() {
        return String::new();
    }
    let mut output = "    _fr_issues = []\n    _fr_inputs = {}\n".to_owned();
    if route
        .inputs
        .iter()
        .any(|input| input.source == HttpInputSource::JsonBody)
    {
        output.push_str("    try:\n        _fr_body = await request.json()\n    except Exception:\n        _fr_body = None\n    _fr_body = _fr_body if isinstance(_fr_body, dict) else {}\n");
    }
    for input in &route.inputs {
        let raw = match input.source {
            HttpInputSource::Query => {
                format!("_fr_query_input(request, {})", quoted(&input.name))
            }
            HttpInputSource::JsonBody => format!("_fr_body.get({})", quoted(&input.name)),
        };
        output.push_str(&format!(
            "    _fr_inputs[{}] = _fr_read_input({raw}, {}, {}, {}, _fr_issues)\n",
            quoted(&input.name),
            quoted(source(input.source)),
            quoted(&input.name),
            quoted(scalar(input.scalar)),
        ));
    }
    output.push_str("    if _fr_issues:\n        return JSONResponse(content={\"error\": \"validation\", \"issues\": _fr_issues}, status_code=422)\n");
    output
}

fn typescript_request(route: &HttpRoute, request: &str, nextjs: bool) -> String {
    if route.inputs.is_empty() {
        return String::new();
    }
    let indent = "  ";
    let mut output = format!(
        "{indent}const _fr_issues: FrIssue[] = [];\n{indent}const _fr_inputs = Object.create(null) as Record<string, unknown>;\n"
    );
    if route
        .inputs
        .iter()
        .any(|input| input.source == HttpInputSource::JsonBody)
    {
        if nextjs {
            output.push_str(&format!(
                "{indent}let _fr_body: unknown;\n{indent}try {{ _fr_body = await {request}.json(); }} catch {{ _fr_body = undefined; }}\n"
            ));
        } else {
            output.push_str(&format!(
                "{indent}const _fr_body: unknown = {request}.body;\n"
            ));
        }
        output.push_str(&format!(
            "{indent}const _fr_body_object: Record<string, unknown> = typeof _fr_body === \"object\" && _fr_body !== null && !Array.isArray(_fr_body) ? _fr_body as Record<string, unknown> : {{}};\n"
        ));
    }
    for input in &route.inputs {
        let raw = match input.source {
            HttpInputSource::Query if nextjs => format!(
                "((_fr_values) => _fr_values.length === 1 ? _fr_values[0] : undefined)(new URL({request}.url).searchParams.getAll({}))",
                quoted(&input.name),
            ),
            HttpInputSource::Query => format!("{request}.query[{}]", quoted(&input.name)),
            HttpInputSource::JsonBody => {
                format!("_fr_body_object[{}]", quoted(&input.name))
            }
        };
        output.push_str(&format!(
            "{indent}_fr_inputs[{}] = frReadInput({raw}, {}, {}, {}, _fr_issues);\n",
            quoted(&input.name),
            quoted(source(input.source)),
            quoted(&input.name),
            quoted(scalar(input.scalar)),
        ));
    }
    output
}

fn go_request(route: &HttpRoute) -> String {
    if route.inputs.is_empty() {
        return String::new();
    }
    let mut output = "\t\t_fr_inputs := map[string]any{}\n\t\t_fr_issues := []any{}\n".to_owned();
    if route
        .inputs
        .iter()
        .any(|input| input.source == HttpInputSource::JsonBody)
    {
        output.push_str("\t\t_fr_body := map[string]any{}\n\t\t_fr_decoder := json.NewDecoder(r.Body)\n\t\t_fr_decoder.UseNumber()\n\t\tif err := _fr_decoder.Decode(&_fr_body); err != nil { _fr_body = map[string]any{} }\n");
    }
    for (index, input) in route.inputs.iter().enumerate() {
        let (raw, present) = match input.source {
            HttpInputSource::Query => {
                output.push_str(&format!(
                    "\t\t_fr_raw_{index}, _fr_present_{index} := frQueryInput(r.URL.Query()[{}])\n",
                    quoted(&input.name)
                ));
                (format!("_fr_raw_{index}"), format!("_fr_present_{index}"))
            }
            HttpInputSource::JsonBody => (
                format!("_fr_body[{}]", quoted(&input.name)),
                format!("_fr_body[{}] != nil", quoted(&input.name)),
            ),
        };
        output.push_str(&format!(
            "\t\t_fr_value_{index}, _fr_issue_{index} := frReadInput({raw}, {present}, {}, {}, {})\n\t\tif _fr_issue_{index} != nil {{ _fr_issues = append(_fr_issues, _fr_issue_{index}) }} else {{ _fr_inputs[{}] = _fr_value_{index} }}\n",
            quoted(source(input.source)),
            quoted(&input.name),
            quoted(scalar(input.scalar)),
            quoted(&input.name),
        ));
    }
    output.push_str("\t\tif len(_fr_issues) > 0 {\n\t\t\tw.Header().Set(\"Content-Type\", \"application/json\")\n\t\t\tw.WriteHeader(422)\n\t\t\t_ = json.NewEncoder(w).Encode(map[string]any{\"error\": \"validation\", \"issues\": _fr_issues})\n\t\t\treturn\n\t\t}\n");
    output
}

pub fn write_routes(
    routes: &[HttpRoute],
    adapter: Adapter,
) -> Result<BTreeMap<String, String>, String> {
    super::validate_routes(routes)?;
    if adapter == Adapter::React {
        return Err("React does not provide an HTTP route adapter.".into());
    }
    let has_inputs = routes.iter().any(|route| !route.inputs.is_empty());
    let has_body = routes.iter().any(|route| {
        route
            .inputs
            .iter()
            .any(|input| input.source == HttpInputSource::JsonBody)
    });
    let mut files = BTreeMap::new();
    let mut body = match adapter {
        Adapter::Fastapi => format!(
            "{}from fastapi import APIRouter, Path{}\nfrom fastapi.responses import JSONResponse\n\nrouter = APIRouter()\n{}",
            if has_inputs { "import math\n" } else { "" },
            if has_inputs { ", Request" } else { "" },
            if has_inputs { PYTHON_VALIDATION } else { "" }
        ),
        Adapter::Express => format!(
            "import {}{{ Router, type Request, type Response }} from \"express\";\n\nconst router = Router();\n{}export default router;\n{}",
            if has_body { "express, " } else { "" },
            if has_body {
                "router.use(express.json({ strict: true, limit: \"1mb\" }));\n"
            } else {
                ""
            },
            if has_inputs { TYPESCRIPT_VALIDATION } else { "" }
        ),
        Adapter::GoNetHttp => format!(
            "package frgenerated\n\nimport (\n\t\"encoding/json\"\n\t\"net/http\"\n{} )\n\n{}func Handler() http.Handler {{\n\tmux := http.NewServeMux()\n",
            if has_inputs { "\t\"strconv\"\n" } else { "" },
            if has_inputs { GO_VALIDATION } else { "" }
        )
        .replace("\n )", "\n)"),
        _ => String::new(),
    };
    for (index, route) in routes.iter().enumerate() {
        let parameters = route.parameters()?;
        let mut bindings: BTreeMap<_, _> = parameters
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let value = match adapter {
                    Adapter::Fastapi => format!("_fr_parameter_{index}"),
                    Adapter::Express => format!("req.params[{}]", quoted(name)),
                    Adapter::GoNetHttp => format!("r.PathValue({})", quoted(name)),
                    _ => format!("params[{}]", quoted(name)),
                };
                (name.clone(), value)
            })
            .collect();
        for input in &route.inputs {
            let value = match adapter {
                Adapter::Fastapi | Adapter::Express | Adapter::GoNetHttp | Adapter::Nextjs => {
                    format!("_fr_inputs[{}]", quoted(&input.name))
                }
                Adapter::React => unreachable!(),
            };
            bindings.insert(input.name.clone(), value);
        }
        let response = expression(&route.response, adapter, &bindings);
        match adapter {
            Adapter::Fastapi => {
                let mut params = if route.inputs.is_empty() {
                    Vec::new()
                } else {
                    vec!["request: Request".into()]
                };
                params.extend(parameters.iter().enumerate().map(|(i, name)| {
                    format!("_fr_parameter_{i}: str = Path(alias={})", quoted(name))
                }));
                let asynchronous = if route.inputs.is_empty() {
                    ""
                } else {
                    "async "
                };
                let request = python_request(route);
                body.push_str(&format!("\n@router.{}({})\n{asynchronous}def route_{index}({}):\n{request}    return JSONResponse(content={response}, status_code={})\n", route.method.to_lowercase(), quoted(&route.path), params.join(", "), route.status));
            }
            Adapter::Express => {
                let path = route
                    .path
                    .split('/')
                    .map(|segment| {
                        if let Some(name) = segment
                            .strip_prefix('{')
                            .and_then(|value| value.strip_suffix('}'))
                        {
                            format!(":{name}")
                        } else {
                            segment.into()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let request = typescript_request(route, "req", false);
                let invalid = if route.inputs.is_empty() {
                    ""
                } else {
                    "  if (_fr_issues.length > 0) return res.status(422).json({ error: \"validation\", issues: _fr_issues });\n"
                };
                body.push_str(&format!("\nrouter.{}({}, async (req: Request, res: Response) => {{\n{request}{invalid}  res.status({}).json({response});\n}});\n", route.method.to_lowercase(), quoted(&path), route.status));
            }
            Adapter::GoNetHttp => {
                let path = if route.path == "/" {
                    "/{$}"
                } else {
                    &route.path
                };
                let request = go_request(route);
                body.push_str(&format!("\tmux.HandleFunc({}, func(w http.ResponseWriter, r *http.Request) {{\n{request}\t\tw.Header().Set(\"Content-Type\", \"application/json\")\n\t\tw.WriteHeader({})\n\t\t_ = json.NewEncoder(w).Encode({response})\n\t}})\n", quoted(&format!("{} {}", route.method, path)), route.status));
            }
            Adapter::Nextjs => {
                let segments = route
                    .path
                    .trim_matches('/')
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| {
                        if segment.starts_with('{') {
                            format!("[{}]", &segment[1..segment.len() - 1])
                        } else {
                            segment.into()
                        }
                    })
                    .collect::<Vec<_>>();
                let path = if segments.is_empty() {
                    "route.ts".into()
                } else {
                    format!("{}/route.ts", segments.join("/"))
                };
                let signature = if parameters.is_empty() && route.inputs.is_empty() {
                    String::new()
                } else {
                    let ty = parameters
                        .iter()
                        .map(|name| format!("{}: string", quoted(name)))
                        .collect::<Vec<_>>()
                        .join("; ");
                    if parameters.is_empty() {
                        "request: Request".into()
                    } else {
                        format!(
                            "{}: Request, context: {{ params: Promise<{{ {ty} }}> }}",
                            if route.inputs.is_empty() {
                                "_request"
                            } else {
                                "request"
                            }
                        )
                    }
                };
                let params = if parameters.is_empty() {
                    ""
                } else {
                    "  const params = await context.params;\n"
                };
                let request = typescript_request(route, "request", true);
                let invalid = if route.inputs.is_empty() {
                    ""
                } else {
                    "  if (_fr_issues.length > 0) return Response.json({ error: \"validation\", issues: _fr_issues }, { status: 422 });\n"
                };
                let method = format!("export async function {}({signature}) {{\n{params}{request}{invalid}  return Response.json({response}, {{ status: {} }});\n}}\n", route.method, route.status);
                let file = files.entry(path).or_insert_with(String::new);
                if !route.inputs.is_empty() && !file.contains("type FrInputKind") {
                    file.push_str(TYPESCRIPT_VALIDATION);
                }
                file.push_str(&method);
            }
            Adapter::React => unreachable!(),
        }
    }
    match adapter {
        Adapter::Fastapi => {
            files.insert("routes.py".into(), body);
        }
        Adapter::Express => {
            files.insert("routes.ts".into(), body);
        }
        Adapter::GoNetHttp => {
            body.push_str("\treturn mux\n}\n");
            files.insert("routes.go".into(), body);
        }
        _ => (),
    }
    Ok(files)
}
