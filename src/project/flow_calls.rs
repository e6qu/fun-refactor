use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    PositionalOnly,
    Either,
    KeywordOnly,
}

pub(super) struct Parameter<'t> {
    pub node: Node<'t>,
    pub kind: Kind,
    pub default: Option<Node<'t>>,
}

#[derive(Clone, Serialize)]
pub(super) struct ParameterSignature {
    name: String,
    kind: &'static str,
    site: Occurrence,
    default: Option<Occurrence>,
}

fn literal_default(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "none" | "true" | "false" | "integer" | "float" => true,
        "string" => {
            let text = &source[node.byte_range()];
            let prefix = text.split(['\'', '"']).next().unwrap_or_default();
            !prefix.to_ascii_lowercase().contains('f')
                && !node
                    .named_children(&mut node.walk())
                    .any(|n| n.kind() == "interpolation")
        }
        "parenthesized_expression" => {
            let children: Vec<_> = node
                .named_children(&mut node.walk())
                .filter(|n| n.kind() != "comment")
                .collect();
            children.len() == 1 && literal_default(children[0], source)
        }
        "unary_operator" => {
            let operator = node.child_by_field_name("operator");
            let argument = node.child_by_field_name("argument");
            operator.is_some_and(|n| matches!(&source[n.byte_range()], "+" | "-"))
                && argument.is_some_and(|n| matches!(n.kind(), "integer" | "float"))
        }
        _ => false,
    }
}

pub(super) fn parameters<'t>(function: Node<'t>, source: &str) -> Option<Vec<Parameter<'t>>> {
    let list = function.child_by_field_name("parameters")?;
    let mut result: Vec<Parameter<'t>> = Vec::new();
    let mut names = BTreeSet::new();
    let mut slash = false;
    let mut star = false;
    let mut optional = false;
    for node in list.named_children(&mut list.walk()) {
        match node.kind() {
            "comment" => (),
            "identifier" | "default_parameter" => {
                let (name, default) = if node.kind() == "default_parameter" {
                    let name = node.child_by_field_name("name")?;
                    let value = node.child_by_field_name("value")?;
                    if name.kind() != "identifier" || !literal_default(value, source) {
                        return None;
                    }
                    (name, Some(value))
                } else {
                    (node, None)
                };
                if !source[name.byte_range()].is_ascii()
                    || !names.insert(&source[name.byte_range()])
                    || !star && optional && default.is_none()
                {
                    return None;
                }
                optional |= default.is_some();
                result.push(Parameter {
                    node: name,
                    kind: if star {
                        Kind::KeywordOnly
                    } else {
                        Kind::Either
                    },
                    default,
                });
            }
            "positional_separator" if !slash && !star && !result.is_empty() => {
                slash = true;
                for parameter in &mut result {
                    parameter.kind = Kind::PositionalOnly;
                }
            }
            "keyword_separator" if !star => star = true,
            _ => return None,
        }
    }
    if star && result.last().is_none_or(|p| p.kind != Kind::KeywordOnly) {
        return None;
    }
    Some(result)
}

pub(super) fn contract() -> Value {
    json!({"schema":"fr-call-binding-2",
        "parameters":"ASCII-named positional-only, positional-or-keyword and keyword-only parameters; required or immutable literal defaults.",
        "evaluation":"explicit argument values in source order before parameter binding; once per transfer.",
        "binding":"positional slots followed by exact keyword names; substitute in declaration order.",
        "invalid":"incomplete analysis for missing, excess, duplicate or unknown arguments; no TypeError model.",
        "boundary":"no evaluated or mutable defaults, annotations, variadics, unpacking, keyword external-rule contracts or non-normalized identifier spellings.",
        "defaults":"omitted slots use source-free immutable scalar literals; explicit arguments override defaults; analysis refuses definition-time effects.",
        "implicit_exceptions":false})
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn parameter_nodes(&self, function: Node<'t>) -> Option<Vec<Node<'t>>> {
        if self.solver.enabled {
            parameters(function, self.source)
                .map(|items| items.into_iter().map(|p| p.node).collect())
        } else {
            let list = function.child_by_field_name("parameters")?;
            let items: Vec<_> = list.named_children(&mut list.walk()).collect();
            items
                .iter()
                .all(|p| p.kind() == "identifier")
                .then_some(items)
        }
    }

    pub(super) fn call_arguments(
        &mut self,
        node: Node<'t>,
        env: &Environment,
    ) -> (Vec<Flow>, Vec<Option<String>>) {
        let mut values = Vec::new();
        let mut names = Vec::new();
        let mut keywords = false;
        if let Some(list) = node.child_by_field_name("arguments") {
            for arg in list.named_children(&mut list.walk()) {
                if arg.kind() == "comment" {
                    continue;
                }
                if self.solver.enabled && !self.normal {
                    break;
                }
                let (value, name) = if self.solver.enabled && arg.kind() == "keyword_argument" {
                    let (Some(name), Some(value)) = (
                        arg.child_by_field_name("name"),
                        arg.child_by_field_name("value"),
                    ) else {
                        self.cutoff("malformed-keyword-argument");
                        break;
                    };
                    if !self.text(name).is_ascii() {
                        self.cutoff("non-ascii-keyword-binding-unchecked");
                    }
                    keywords = true;
                    (value, Some(self.text(name).to_owned()))
                } else {
                    if matches!(arg.kind(), "list_splat" | "dictionary_splat") {
                        self.cutoff("argument-unpacking-unchecked");
                        break;
                    }
                    if keywords {
                        self.cutoff("positional-after-keyword");
                    }
                    (arg, None)
                };
                values.push(self.expression(value, env));
                names.push(name);
            }
        }
        (values, names)
    }

    pub(super) fn bind_arguments(
        &mut self,
        name: &str,
        function: Node<'t>,
        values: Vec<Flow>,
        names: &[Option<String>],
    ) -> Option<Vec<Flow>> {
        let source = self.contexts[name].1;
        let Some(parameters) = parameters(function, source) else {
            self.cutoff(format!("unsupported-parameter-contract:{name}"));
            return None;
        };
        let mut slots: Vec<Option<Flow>> = vec![None; parameters.len()];
        let mut position = 0;
        for (value, keyword) in values.into_iter().zip(names) {
            let slot = if let Some(keyword) = keyword {
                parameters.iter().position(|p| {
                    p.kind != Kind::PositionalOnly && &source[p.node.byte_range()] == keyword
                })
            } else {
                let next = parameters
                    .get(position)
                    .filter(|p| p.kind != Kind::KeywordOnly)
                    .map(|_| position);
                position += 1;
                next
            };
            let Some(slot) = slot else {
                self.cutoff(format!("invalid-call-binding:{name}"));
                return None;
            };
            if slots[slot].replace(value).is_some() {
                self.cutoff(format!("invalid-call-binding:{name}"));
                return None;
            }
        }
        for (slot, parameter) in slots.iter_mut().zip(&parameters) {
            if slot.is_none() && parameter.default.is_some() {
                *slot = Some(Flow::new());
            }
        }
        let result: Option<Vec<_>> = slots.into_iter().collect();
        if result.is_none() {
            self.cutoff(format!("invalid-call-binding:{name}"));
        }
        result
    }
}

pub(super) fn valid_call_syntax(node: Node<'_>, source: &str) -> bool {
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return false;
    };
    if arguments.kind() != "argument_list" {
        return false;
    }
    let mut keywords = BTreeSet::new();
    let mut dictionary = false;
    for arg in arguments.named_children(&mut arguments.walk()) {
        match arg.kind() {
            "comment" => (),
            "keyword_argument" => {
                let Some(name) = arg.child_by_field_name("name") else {
                    return false;
                };
                let name = &source[name.byte_range()];
                if !name.is_ascii() || !keywords.insert(name) {
                    return false;
                }
            }
            "dictionary_splat" => dictionary = true,
            "list_splat" if !dictionary => (),
            _ if dictionary || !keywords.is_empty() => return false,
            _ => (),
        }
    }
    true
}

impl Analyzer<'_, '_, '_> {
    pub(super) fn parameter_signature(&self, function: Node<'_>) -> Vec<ParameterSignature> {
        parameters(function, self.source)
            .unwrap_or_default()
            .into_iter()
            .map(|parameter| ParameterSignature {
                name: self.text(parameter.node).to_owned(),
                kind: match parameter.kind {
                    Kind::PositionalOnly => "positional-only",
                    Kind::Either => "positional-or-keyword",
                    Kind::KeywordOnly => "keyword-only",
                },
                site: self.occurrence(parameter.node, "summary-parameter"),
                default: parameter
                    .default
                    .map(|node| self.occurrence(node, "parameter-default")),
            })
            .collect()
    }
}
