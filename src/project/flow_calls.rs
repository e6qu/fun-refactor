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
}

pub(super) fn parameters<'t>(function: Node<'t>, source: &str) -> Option<Vec<Parameter<'t>>> {
    let list = function.child_by_field_name("parameters")?;
    let mut result: Vec<Parameter<'t>> = Vec::new();
    let mut names = BTreeSet::new();
    let mut slash = false;
    let mut star = false;
    for node in list.named_children(&mut list.walk()) {
        match node.kind() {
            "comment" => (),
            "identifier" => {
                if !source[node.byte_range()].is_ascii()
                    || !names.insert(&source[node.byte_range()])
                {
                    return None;
                }
                result.push(Parameter {
                    node,
                    kind: if star {
                        Kind::KeywordOnly
                    } else {
                        Kind::Either
                    },
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
    json!({"schema":"fr-call-binding-1",
        "parameters":"required ASCII-named positional-only, positional-or-keyword and keyword-only parameters.",
        "evaluation":"explicit argument values in source order before parameter binding; once per transfer.",
        "binding":"positional slots followed by exact keyword names; substitute in declaration order.",
        "invalid":"incomplete analysis for missing, excess, duplicate or unknown arguments; no TypeError model.",
        "boundary":"no defaults, annotations, variadics, unpacking, keyword external-rule contracts or Unicode binding normalization.",
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
