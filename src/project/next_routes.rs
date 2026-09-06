use super::contracts::simple_name;
use crate::parse::Parsed;
use crate::transpile::routes::Endpoint;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tree_sitter::Node;

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(super) struct NextRoute {
    pub endpoint: Endpoint,
    pub name_offset: usize,
    pub basis: &'static str,
    pub catch_all: Option<CatchAll>,
}

#[derive(Clone)]
pub(super) struct CatchAll {
    pub name: String,
    pub optional: bool,
}

struct RoutePath {
    url: String,
    catch_all: Option<CatchAll>,
}

#[derive(Default)]
pub(super) struct NextRoutes {
    pub entries: Vec<NextRoute>,
    pub gaps: Vec<(usize, &'static str)>,
}

fn route_url(path: &Path) -> Option<Result<RoutePath, &'static str>> {
    let relative = path
        .strip_prefix("app")
        .or_else(|_| path.strip_prefix("src/app"))
        .ok()?;
    if !matches!(relative.file_name()?.to_str()?, "route.ts" | "route.js") {
        return None;
    }
    let mut segments = Vec::new();
    let mut catch_all = None;
    let mut names = BTreeSet::new();
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
        if catch_all.is_some() {
            return Some(Err("Only terminal catch-all segments are supported; later URL segments need further inspection."));
        }
        let rest = part
            .strip_prefix("[[...")
            .and_then(|s| s.strip_suffix("]]"))
            .map(|name| (name, true))
            .or_else(|| {
                part.strip_prefix("[...")
                    .and_then(|s| s.strip_suffix(']'))
                    .map(|name| (name, false))
            });
        if let Some((name, optional)) = rest {
            if !simple_name(name) || !names.insert(name.to_owned()) {
                return Some(Err(
                    "Malformed or repeated dynamic parameter names need further inspection.",
                ));
            }
            catch_all = Some(CatchAll {
                name: name.to_owned(),
                optional,
            });
            segments.push(format!("{{...{name}{}}}", if optional { "?" } else { "" }));
            continue;
        }
        if let Some(name) = part.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if !simple_name(name) || !names.insert(name.to_owned()) {
                return Some(Err(
                    "Malformed or repeated dynamic parameter names need further inspection.",
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
    Some(Ok(RoutePath {
        url: format!("/{}", segments.join("/")),
        catch_all,
    }))
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| !n.is_extra())
        .collect()
}

fn has_token(node: Node<'_>, token: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|n| n.kind() == token);
    found
}

impl NextRoutes {
    fn push(
        &mut self,
        function: Node<'_>,
        method: &str,
        line: usize,
        path: &RoutePath,
        basis: &'static str,
        source: &str,
    ) {
        if let Some(name) = function.child_by_field_name("name") {
            self.entries.push(NextRoute {
                endpoint: Endpoint {
                    method: method.to_owned(),
                    url: path.url.clone(),
                    handler: Some(source[name.byte_range()].to_owned()),
                    line,
                },
                name_offset: name.start_byte(),
                basis,
                catch_all: path.catch_all.clone(),
            });
        }
    }
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
    let functions: Vec<_> = named_children(parsed.root())
        .into_iter()
        .filter_map(|node| {
            if node.kind() == "export_statement" {
                node.child_by_field_name("declaration")
            } else {
                Some(node)
            }
        })
        .filter(|n| n.kind() == "function_declaration")
        .collect();
    for export in named_children(parsed.root())
        .into_iter()
        .filter(|n| n.kind() == "export_statement")
    {
        let declaration = export.child_by_field_name("declaration");
        if has_token(export, "type") {
            continue;
        }
        if has_token(export, "default") {
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
                    result.push(
                        function,
                        method,
                        export.start_position().row + 1,
                        &url,
                        "nextjs-app-function-export",
                        source,
                    );
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
            for specifier in named_children(clause) {
                if has_token(specifier, "type") {
                    continue;
                }
                let Some(name) = specifier.child_by_field_name("name") else {
                    continue;
                };
                let alias = specifier.child_by_field_name("alias").unwrap_or(name);
                let method = &source[alias.byte_range()];
                if !METHODS.contains(&method.trim_matches(['\'', '"'])) {
                    continue;
                }
                if export.child_by_field_name("source").is_some()
                    || name.kind() != "identifier"
                    || alias.kind() != "identifier"
                {
                    bindings.push(alias);
                    continue;
                }
                let matches: Vec<_> = functions
                    .iter()
                    .filter(|function| {
                        function
                            .child_by_field_name("name")
                            .is_some_and(|n| source[n.byte_range()] == source[name.byte_range()])
                    })
                    .collect();
                if matches.is_empty() {
                    bindings.push(alias);
                }
                for function in matches {
                    result.push(
                        *function,
                        method,
                        specifier.start_position().row + 1,
                        &url,
                        "nextjs-app-local-function-export",
                        source,
                    );
                }
            }
        }
        let unsupported = bindings
            .iter()
            .any(|node| METHODS.contains(&source[node.byte_range()].trim_matches(['\'', '"'])));
        let mut cursor = export.walk();
        if unsupported || export.children(&mut cursor).any(|n| n.kind() == "*") {
            result.gaps.push((export.start_position().row + 1, "Variable handlers, cross-file re-exports and unresolved local exports need further inspection; only direct function declarations supply handler evidence."));
        }
    }
    let mut counts = BTreeMap::new();
    for entry in &result.entries {
        *counts.entry(&entry.endpoint.method).or_insert(0usize) += 1;
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
