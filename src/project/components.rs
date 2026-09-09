use crate::parse::Parsed;
use std::collections::BTreeMap;
use std::path::{Component as PathComponent, Path, PathBuf};
use tree_sitter::Node;

pub(super) struct Component {
    pub name: Option<String>,
    pub line: usize,
    pub client: bool,
    pub props: Vec<String>,
    pub props_type: Option<String>,
    pub states: Vec<State>,
    pub effects: Vec<Effect>,
    pub events: Vec<Event>,
    pub styles: Vec<Style>,
    pub renders: Vec<Render>,
    pub hooks: Vec<Hook>,
    pub default_export: bool,
}

pub(super) struct Hook {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
}

#[derive(Clone)]
pub(super) struct Import {
    pub local: String,
    pub imported: String,
    pub source: String,
    pub line: usize,
}

pub(super) struct State {
    pub hook: String,
    pub binding: Option<String>,
    pub setter: Option<String>,
    pub line: usize,
}

pub(super) struct Effect {
    pub hook: String,
    pub schedule: &'static str,
    pub dependency_count: Option<usize>,
    pub cleanup_candidate: bool,
    pub line: usize,
}

fn effect_cleanup(arguments: &[Node<'_>]) -> bool {
    let Some(setup) = arguments.first().copied() else {
        return false;
    };
    let mut stack = vec![setup];
    while let Some(node) = stack.pop() {
        if node.kind() == "return_statement"
            && children(node)
                .into_iter()
                .any(|value| matches!(value.kind(), "arrow_function" | "function_expression"))
        {
            return true;
        }
        stack.extend(children(node));
    }
    false
}

pub(super) struct Event {
    pub name: String,
    pub element: Option<String>,
    pub handler_kind: &'static str,
    pub line: usize,
}

pub(super) struct Style {
    pub attribute: String,
    pub value_kind: &'static str,
    pub line: usize,
}

pub(super) struct Render {
    pub target: String,
    pub line: usize,
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.is_extra())
        .collect()
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn has_jsx(node: Node<'_>) -> bool {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind().starts_with("jsx_") {
            return true;
        }
        stack.extend(children(node));
    }
    false
}

pub(super) fn client_module(parsed: &Parsed, source: &str) -> bool {
    children(parsed.root()).first().is_some_and(|node| {
        node.kind() == "expression_statement"
            && matches!(
                text(*node, source).trim().trim_end_matches(';'),
                "\"use client\"" | "'use client'"
            )
    })
}

fn component_nodes<'a>(parsed: &'a Parsed, source: &str) -> Vec<(Node<'a>, Option<String>, bool)> {
    let mut result = Vec::new();
    for item in children(parsed.root()) {
        let node = if item.kind() == "export_statement" {
            item.child_by_field_name("declaration").unwrap_or(item)
        } else {
            item
        };
        if node.kind() == "function_declaration" && has_jsx(node) {
            let name = node
                .child_by_field_name("name")
                .map(|name| text(name, source).to_owned());
            if name
                .as_deref()
                .is_none_or(|name| name.chars().next().is_some_and(char::is_uppercase))
            {
                result.push((
                    node,
                    name,
                    item.kind() == "export_statement"
                        && text(item, source)
                            .trim_start()
                            .starts_with("export default"),
                ));
            }
        } else if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
            for binding in children(node)
                .into_iter()
                .filter(|child| child.kind() == "variable_declarator")
            {
                let (Some(name), Some(value)) = (
                    binding.child_by_field_name("name"),
                    binding.child_by_field_name("value"),
                ) else {
                    continue;
                };
                let name = text(name, source);
                if name.chars().next().is_some_and(char::is_uppercase)
                    && matches!(value.kind(), "arrow_function" | "function_expression")
                    && has_jsx(value)
                {
                    result.push((value, Some(name.to_owned()), false));
                }
            }
        }
    }
    result
}

fn props(function: Node<'_>, source: &str) -> (Vec<String>, Option<String>) {
    let parameters = function
        .child_by_field_name("parameters")
        .map(children)
        .unwrap_or_else(|| {
            function
                .child_by_field_name("parameter")
                .into_iter()
                .collect()
        });
    let Some(parameter) = parameters.first().copied() else {
        return (Vec::new(), None);
    };
    let ty = parameter
        .child_by_field_name("type")
        .map(|ty| text(ty, source).trim_start_matches(':').trim().to_owned());
    if parameter.kind() != "object_pattern"
        && !children(parameter)
            .iter()
            .any(|node| node.kind() == "object_pattern")
    {
        return (Vec::new(), ty);
    }
    let object = if parameter.kind() == "object_pattern" {
        parameter
    } else {
        children(parameter)
            .into_iter()
            .find(|node| node.kind() == "object_pattern")
            .unwrap()
    };
    let mut names = Vec::new();
    for part in children(object) {
        let candidate = part
            .child_by_field_name("key")
            .or_else(|| part.child_by_field_name("name"))
            .unwrap_or(part);
        if matches!(
            candidate.kind(),
            "identifier" | "shorthand_property_identifier_pattern"
        ) {
            names.push(text(candidate, source).to_owned());
        }
    }
    names.sort();
    names.dedup();
    (names, ty)
}

fn attribute_name(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| {
            children(node)
                .into_iter()
                .find(|child| child.kind().contains("identifier"))
        })
        .map(|name| text(name, source).to_owned())
}

fn inspect(function: Node<'_>, source: &str, component: &mut Component) {
    let mut stack = vec![function];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            if let Some(function) = node.child_by_field_name("function") {
                let hook = text(function, source);
                if matches!(hook, "useState" | "useReducer") {
                    let declarator = node.parent().and_then(|parent| {
                        (parent.kind() == "variable_declarator").then_some(parent)
                    });
                    let bindings = declarator
                        .and_then(|declaration| declaration.child_by_field_name("name"))
                        .filter(|name| name.kind() == "array_pattern")
                        .map(children)
                        .unwrap_or_default();
                    component.states.push(State {
                        hook: hook.to_owned(),
                        binding: bindings.first().map(|name| text(*name, source).to_owned()),
                        setter: bindings.get(1).map(|name| text(*name, source).to_owned()),
                        line: node.start_position().row + 1,
                    });
                } else if matches!(hook, "useEffect" | "useLayoutEffect" | "useInsertionEffect") {
                    let arguments = node
                        .child_by_field_name("arguments")
                        .map(children)
                        .unwrap_or_default();
                    let dependencies = arguments.get(1).copied();
                    let dependency_count = dependencies
                        .filter(|value| value.kind() == "array")
                        .map(|value| children(value).len());
                    let schedule = if dependencies.is_none() {
                        "every-commit-candidate"
                    } else if dependency_count == Some(0) {
                        "mount-candidate"
                    } else if dependency_count.is_some() {
                        "dependency-change-candidate"
                    } else {
                        "dynamic-candidate"
                    };
                    component.effects.push(Effect {
                        hook: hook.to_owned(),
                        schedule,
                        dependency_count,
                        cleanup_candidate: effect_cleanup(&arguments),
                        line: node.start_position().row + 1,
                    });
                } else if hook
                    .strip_prefix("use")
                    .and_then(|suffix| suffix.chars().next())
                    .is_some_and(char::is_uppercase)
                {
                    component.hooks.push(Hook {
                        name: hook.to_owned(),
                        kind: if matches!(
                            hook,
                            "useActionState"
                                | "useCallback"
                                | "useContext"
                                | "useDebugValue"
                                | "useDeferredValue"
                                | "useId"
                                | "useImperativeHandle"
                                | "useMemo"
                                | "useOptimistic"
                                | "useRef"
                                | "useSyncExternalStore"
                                | "useTransition"
                        ) {
                            "react-hook-candidate"
                        } else {
                            "custom-hook-candidate"
                        },
                        line: node.start_position().row + 1,
                    });
                }
            }
        } else if node.kind() == "jsx_attribute" {
            let name_node = node.child_by_field_name("name").or_else(|| {
                children(node)
                    .into_iter()
                    .find(|child| child.kind().contains("identifier"))
            });
            let Some(name) = attribute_name(node, source) else {
                stack.extend(children(node));
                continue;
            };
            let value = node.child_by_field_name("value").or_else(|| {
                children(node)
                    .into_iter()
                    .find(|child| Some(*child) != name_node)
            });
            if name.starts_with("on") {
                let handler_kind = match value.map(|node| node.kind()) {
                    Some("jsx_expression") => value
                        .and_then(|value| children(value).first().copied())
                        .map_or("empty", |expression| match expression.kind() {
                            "identifier" | "member_expression" => "reference",
                            "arrow_function" | "function_expression" => "inline",
                            _ => "expression",
                        }),
                    Some(_) => "literal",
                    None => "empty",
                };
                component.events.push(Event {
                    name,
                    element: node.parent().and_then(|parent| {
                        parent
                            .child_by_field_name("name")
                            .map(|name| text(name, source).to_owned())
                    }),
                    handler_kind,
                    line: node.start_position().row + 1,
                });
            } else if matches!(name.as_str(), "class" | "className" | "style") {
                component.styles.push(Style {
                    attribute: name,
                    value_kind: match value.map(|node| node.kind()) {
                        Some("string") => "literal",
                        Some("jsx_expression") => "expression",
                        Some(_) => "other",
                        None => "empty",
                    },
                    line: node.start_position().row + 1,
                });
            }
        } else if matches!(
            node.kind(),
            "jsx_opening_element" | "jsx_self_closing_element"
        ) {
            if let Some(name) = node.child_by_field_name("name") {
                let target = text(name, source);
                if target.chars().next().is_some_and(char::is_uppercase) {
                    component.renders.push(Render {
                        target: target.to_owned(),
                        line: node.start_position().row + 1,
                    });
                }
            }
        }
        stack.extend(children(node));
    }
}

pub(super) fn read(parsed: &Parsed, source: &str) -> Vec<Component> {
    let client = client_module(parsed, source);
    component_nodes(parsed, source)
        .into_iter()
        .map(|(function, name, default_export)| {
            let (props, props_type) = props(function, source);
            let mut component = Component {
                name,
                line: function.start_position().row + 1,
                client,
                props,
                props_type,
                states: Vec::new(),
                effects: Vec::new(),
                events: Vec::new(),
                styles: Vec::new(),
                renders: Vec::new(),
                hooks: Vec::new(),
                default_export,
            };
            inspect(function, source, &mut component);
            component
        })
        .collect()
}

fn string_literal(node: Node<'_>, source: &str) -> Option<String> {
    let raw = text(node, source);
    let quote = raw.chars().next()?;
    if !matches!(quote, '\'' | '"') || raw.contains(['\\', '\n', '\r']) {
        return None;
    }
    raw.strip_prefix(quote)
        .and_then(|value| value.strip_suffix(quote))
        .map(str::to_owned)
}

pub(super) fn imports(parsed: &Parsed, source: &str) -> Vec<Import> {
    let mut imports = Vec::new();
    for statement in children(parsed.root())
        .into_iter()
        .filter(|node| node.kind() == "import_statement")
    {
        if text(statement, source)
            .trim_start()
            .starts_with("import type")
        {
            continue;
        }
        let Some(import_source) = statement
            .child_by_field_name("source")
            .and_then(|node| string_literal(node, source))
        else {
            continue;
        };
        let line = statement.start_position().row + 1;
        let Some(clause) = children(statement)
            .into_iter()
            .find(|node| node.kind() == "import_clause")
        else {
            continue;
        };
        for node in children(clause) {
            if node.kind() == "identifier" {
                imports.push(Import {
                    local: text(node, source).to_owned(),
                    imported: "default".to_owned(),
                    source: import_source.clone(),
                    line,
                });
            } else if node.kind() == "named_imports" {
                for specifier in children(node)
                    .into_iter()
                    .filter(|node| node.kind() == "import_specifier")
                {
                    let Some(name) = specifier.child_by_field_name("name") else {
                        continue;
                    };
                    let alias = specifier.child_by_field_name("alias").unwrap_or(name);
                    imports.push(Import {
                        local: text(alias, source).to_owned(),
                        imported: text(name, source).to_owned(),
                        source: import_source.clone(),
                        line,
                    });
                }
            }
        }
    }
    imports
}

fn normalized(base: &Path, source: &str) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for part in base.join(source).components() {
        match part {
            PathComponent::Normal(part) => result.push(part),
            PathComponent::CurDir => {}
            PathComponent::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            PathComponent::Prefix(_) | PathComponent::RootDir => return None,
        }
    }
    Some(result)
}

pub(super) fn import_candidates(
    importer: &Path,
    source: &str,
    sources: &BTreeMap<PathBuf, String>,
    root: &Path,
) -> Vec<PathBuf> {
    if !source.starts_with('.') {
        return Vec::new();
    }
    let Some(base) = normalized(importer.parent().unwrap_or(Path::new("")), source) else {
        return Vec::new();
    };
    let candidates = if base.extension().is_some() {
        vec![base]
    } else {
        vec![
            base.with_extension("tsx"),
            base.with_extension("jsx"),
            base.join("index.tsx"),
            base.join("index.jsx"),
        ]
    };
    candidates
        .into_iter()
        .filter(|path| sources.contains_key(&root.join(path)))
        .collect()
}
