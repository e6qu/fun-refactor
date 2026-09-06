use super::contracts::simple_name;
use crate::parse::Parsed;
use crate::transpile::routes::Endpoint;
use std::collections::BTreeMap;
use std::path::Path;
use tree_sitter::Node;

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(super) struct NextRoute {
    pub endpoint: Endpoint,
    pub name_offset: usize,
}

#[derive(Default)]
pub(super) struct NextRoutes {
    pub entries: Vec<NextRoute>,
    pub gaps: Vec<(usize, &'static str)>,
}

fn route_url(path: &Path) -> Option<Result<String, &'static str>> {
    let relative = path
        .strip_prefix("app")
        .or_else(|_| path.strip_prefix("src/app"))
        .ok()?;
    if !matches!(relative.file_name()?.to_str()?, "route.ts" | "route.js") {
        return None;
    }
    let mut segments = Vec::new();
    for part in relative.parent()?.components() {
        let part = part.as_os_str().to_str()?;
        if part.starts_with('_') {
            return Some(Err(
                "Private directories are outside the public App Router route subset.",
            ));
        }
        if part.starts_with('@') || part.starts_with("(.") {
            return Some(Err(
                "Parallel and intercepting route paths need further inspection.",
            ));
        }
        if let Some(group) = part.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            if group.is_empty() || group.contains(['(', ')']) {
                return Some(Err("The route group exceeds the supported path subset."));
            }
            continue;
        }
        if let Some(name) = part.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if !simple_name(name) {
                return Some(Err(
                    "Catch-all, optional and complex dynamic segments need further inspection.",
                ));
            }
            segments.push(format!("{{{name}}}"));
        } else if !part
            .chars()
            .all(|c| c.is_alphanumeric() || "-._~".contains(c))
        {
            return Some(Err(
                "The route path exceeds the supported literal segment subset.",
            ));
        } else {
            segments.push(part.to_owned());
        }
    }
    Some(Ok(format!("/{}", segments.join("/"))))
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| !n.is_extra())
        .collect()
}

pub(super) fn read(path: &Path, parsed: &Parsed, source: &str) -> NextRoutes {
    let mut result = NextRoutes::default();
    let Some(url) = route_url(path) else {
        return result;
    };
    let url = match url {
        Ok(url) => url,
        Err(reason) => {
            result.gaps.push((1, reason));
            return result;
        }
    };
    let mut counts = BTreeMap::new();
    for export in named_children(parsed.root())
        .into_iter()
        .filter(|n| n.kind() == "export_statement")
    {
        let declaration = export.child_by_field_name("declaration");
        let mut cursor = export.walk();
        if export.children(&mut cursor).any(|n| n.kind() == "default") {
            result.gaps.push((
                export.start_position().row + 1,
                "Default exports do not identify a supported HTTP handler.",
            ));
            continue;
        }
        if let Some(function) = declaration.filter(|n| n.kind() == "function_declaration") {
            if let Some(name) = function.child_by_field_name("name") {
                let method = &source[name.byte_range()];
                if METHODS.contains(&method) {
                    *counts.entry(method).or_insert(0usize) += 1;
                    result.entries.push(NextRoute {
                        endpoint: Endpoint {
                            method: method.to_owned(),
                            url: url.clone(),
                            handler: Some(method.to_owned()),
                            line: export.start_position().row + 1,
                        },
                        name_offset: name.start_byte(),
                    });
                }
            }
            continue;
        }
        let mut bindings = Vec::new();
        if let Some(declaration) = declaration {
            if matches!(
                declaration.kind(),
                "lexical_declaration" | "variable_declaration"
            ) {
                bindings.extend(
                    named_children(declaration)
                        .into_iter()
                        .filter(|n| n.kind() == "variable_declarator")
                        .filter_map(|n| n.child_by_field_name("name")),
                );
            } else if declaration.kind() == "function_signature" {
                bindings.extend(declaration.child_by_field_name("name"));
            }
        }
        for clause in named_children(export)
            .into_iter()
            .filter(|n| n.kind() == "export_clause")
        {
            bindings.extend(named_children(clause).into_iter().filter_map(|n| {
                n.child_by_field_name("alias")
                    .or_else(|| n.child_by_field_name("name"))
            }));
        }
        let unsupported = bindings
            .iter()
            .any(|node| METHODS.contains(&source[node.byte_range()].trim_matches(['\'', '"'])));
        let mut cursor = export.walk();
        if unsupported || export.children(&mut cursor).any(|n| n.kind() == "*") {
            result.gaps.push((export.start_position().row + 1, "Variable handlers and re-exports need further inspection; only named function exports supply handler evidence."));
        }
    }
    if counts.values().any(|count| *count > 1) {
        result.gaps.push((
            1,
            "Multiple declarations export the same HTTP method; runtime validity remains unknown.",
        ));
    }
    if result.entries.is_empty() && result.gaps.is_empty() {
        result.gaps.push((
            1,
            "This route file has no supported named HTTP function exports.",
        ));
    }
    result
}
