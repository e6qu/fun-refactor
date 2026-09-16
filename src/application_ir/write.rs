use super::{Adapter, HttpExpression, HttpRoute};
use std::collections::BTreeMap;

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
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

pub fn write_routes(
    routes: &[HttpRoute],
    adapter: Adapter,
) -> Result<BTreeMap<String, String>, String> {
    super::validate_routes(routes)?;
    if adapter == Adapter::React {
        return Err("React does not provide an HTTP route adapter.".into());
    }
    let mut files = BTreeMap::new();
    let mut body = match adapter {
        Adapter::Fastapi => "from fastapi import APIRouter, Path\nfrom fastapi.responses import JSONResponse\n\nrouter = APIRouter()\n".to_owned(),
        Adapter::Express => "import { Router, type Request, type Response } from \"express\";\n\nconst router = Router();\nexport default router;\n".to_owned(),
        Adapter::GoNetHttp => "package frgenerated\n\nimport (\n\t\"encoding/json\"\n\t\"net/http\"\n)\n\nfunc Handler() http.Handler {\n\tmux := http.NewServeMux()\n".to_owned(),
        _ => String::new(),
    };
    for (index, route) in routes.iter().enumerate() {
        let parameters = route.parameters()?;
        let bindings: BTreeMap<_, _> = parameters
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
        let response = expression(&route.response, adapter, &bindings);
        match adapter {
            Adapter::Fastapi => {
                let params = parameters
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        format!("_fr_parameter_{i}: str = Path(alias={})", quoted(name))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                body.push_str(&format!("\n@router.{}({})\ndef route_{index}({params}):\n    return JSONResponse(content={response}, status_code={})\n", route.method.to_lowercase(), quoted(&route.path), route.status));
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
                body.push_str(&format!("\nrouter.{}({}, (req: Request, res: Response) => {{\n  res.status({}).json({response});\n}});\n", route.method.to_lowercase(), quoted(&path), route.status));
            }
            Adapter::GoNetHttp => {
                let path = if route.path == "/" {
                    "/{$}"
                } else {
                    &route.path
                };
                body.push_str(&format!("\tmux.HandleFunc({}, func(w http.ResponseWriter, r *http.Request) {{\n\t\tw.Header().Set(\"Content-Type\", \"application/json\")\n\t\tw.WriteHeader({})\n\t\t_ = json.NewEncoder(w).Encode({response})\n\t}})\n", quoted(&format!("{} {}", route.method, path)), route.status));
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
                let signature = if parameters.is_empty() {
                    String::new()
                } else {
                    let ty = parameters
                        .iter()
                        .map(|name| format!("{}: string", quoted(name)))
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!("_request: Request, context: {{ params: Promise<{{ {ty} }}> }}")
                };
                let params = if parameters.is_empty() {
                    ""
                } else {
                    "  const params = await context.params;\n"
                };
                let method = format!("export async function {}({signature}) {{\n{params}  return Response.json({response}, {{ status: {} }});\n}}\n", route.method, route.status);
                files
                    .entry(path)
                    .or_insert_with(String::new)
                    .push_str(&method);
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
