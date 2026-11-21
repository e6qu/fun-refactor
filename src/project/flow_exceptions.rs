use super::*;

pub(super) type Raised = BTreeMap<String, Flow>;

pub(super) fn join(into: &mut Raised, from: Raised) -> bool {
    let mut changed = false;
    for (kind, flow) in from {
        if let Some(previous) = into.get_mut(&kind) {
            let count = previous.len();
            merge(previous, flow);
            changed |= previous.len() != count;
        } else {
            into.insert(kind, flow);
            changed = true;
        }
    }
    changed
}

fn known(kind: &str) -> bool {
    matches!(
        kind,
        "IndexError" | "KeyError" | "LookupError" | "ValueError" | "TypeError" | "Exception" | "BaseException"
    )
}

pub(in crate::project) fn valid(value: &Value) -> bool {
    value.as_object().is_some_and(|raised| {
        raised.iter().all(|(kind, flow)| {
            (kind == "unknown" || known(kind))
                && flow.as_object().is_some_and(|flows| {
                    flows.iter().all(|(origin, trace)| {
                        trace["origin"] == *origin && trace["occurrences"].is_array()
                    })
                })
        })
    })
}

fn matches(kind: &str, handler: &str) -> bool {
    kind == handler
        || matches!(kind, "IndexError" | "KeyError") && handler == "LookupError"
        || handler == "BaseException"
        || handler == "Exception" && kind != "BaseException"
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn capture_raised_lists(&mut self, heap: lists::Lists) {
        match &mut self.raised_lists {
            Some(prior) => {
                prior.join(&heap);
            }
            None => self.raised_lists = Some(heap),
        }
    }

    pub(super) fn list_error(
        &mut self,
        node: Node<'t>,
        env: &Environment,
        kind: &str,
        certain: bool,
    ) {
        if !self.solver.enabled {
            self.cutoff("list-exception-control-unchecked");
        }
        self.may_raise = true;
        self.normal &= !certain;
        // Builtin list error text does not contain the stored elements.
        self.exceptional_returns.entry(kind.into()).or_default();
        self.capture_raised_lists(env.lists.borrow().clone());
        self.events
            .push(json!({"kind":"list-error", "exception":kind,
            "certain":certain, "occurrence":self.occurrence(node, "list-error"),
            "control_node":self.current_block}));
    }

    pub(super) fn sequence_error(&mut self, node: Node<'t>, env: &Environment, kind: &str) {
        if !self.solver.enabled {
            self.cutoff("sequence-exception-control-unchecked");
        }
        self.may_raise = true;
        self.normal = false;
        self.exceptional_returns.entry(kind.into()).or_default();
        self.capture_raised_lists(env.lists.borrow().clone());
        self.events
            .push(json!({"kind":"sequence-error", "exception":kind,
            "certain":true, "occurrence":self.occurrence(node, "sequence-error"),
            "control_node":self.current_block}));
    }

    pub(super) fn exception_alias(&self, clause: Node<'t>) -> Option<Node<'t>> {
        let alias = clause.child_by_field_name("alias").or_else(|| {
            clause
                .child_by_field_name("value")
                .filter(|value| value.kind() == "as_pattern")?
                .child_by_field_name("alias")
        })?;
        let name = self.text(alias);
        (matches!(alias.kind(), "identifier" | "as_pattern_target")
            && !name.is_empty()
            && name.bytes().enumerate().all(|(i, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || i > 0 && byte.is_ascii_digit()
            }))
        .then_some(alias)
    }

    pub(super) fn raised_value(
        &mut self,
        node: Node<'t>,
        active: Option<usize>,
        env: &Environment,
        flow: Flow,
    ) -> Raised {
        if node.named_child_count() == 0 {
            if let Some(raised) = active.and_then(|slot| env.pending_raises.get(&slot)) {
                if raised.contains_key("unknown") {
                    self.cutoff("raise-contract-unchecked");
                }
                return raised
                    .iter()
                    .map(|(kind, flow)| (kind.clone(), self.extend(flow.clone(), node, "reraise")))
                    .collect();
            }
            self.cutoff("raise-contract-unchecked");
            return BTreeMap::from([("unknown".into(), flow)]);
        }
        let expression = node.named_child(0).and_then(calls::unparenthesized);
        let kind = expression.and_then(|expression| {
            let class = if expression.kind() == "call" {
                expression.child_by_field_name("function")?
            } else {
                expression
            };
            let resolved = self.standard_call_name(class, env)?;
            let kind = resolved.strip_prefix("builtins.")?;
            known(kind).then(|| kind.to_owned())
        });
        BTreeMap::from([(kind.unwrap_or_else(|| "unknown".into()), flow)])
    }

    pub(super) fn match_exceptions(
        &mut self,
        clause: Node<'t>,
        env: &Environment,
        raised: Raised,
    ) -> (Raised, Raised) {
        let mut cursor = clause.walk();
        let expressions: Vec<_> = clause
            .named_children(&mut cursor)
            .filter(|child| {
                !matches!(child.kind(), "block" | "comment")
                    && Some(*child) != clause.child_by_field_name("alias")
            })
            .map(|child| {
                if child.kind() == "as_pattern" {
                    child.named_child(0).unwrap_or(child)
                } else {
                    child
                }
            })
            .collect();
        let has_alias = clause.child_by_field_name("alias").is_some()
            || clause
                .child_by_field_name("value")
                .is_some_and(|value| value.kind() == "as_pattern");
        let invalid = has_alias && self.exception_alias(clause).is_none()
            || clause
                .children(&mut clause.walk())
                .any(|child| child.kind() == "*");
        let catch_all = !invalid && expressions.is_empty();
        let handlers = if invalid {
            None
        } else if expressions.is_empty() {
            Some(Vec::new())
        } else if expressions.len() == 1 {
            let value = calls::unparenthesized(expressions[0]);
            value
                .filter(|value| calls::initialization_fits(*value))
                .and_then(|value| {
                    let candidates = if value.kind() == "tuple" {
                        value
                            .named_children(&mut value.walk())
                            .filter(|node| node.kind() != "comment")
                            .collect()
                    } else {
                        vec![value]
                    };
                    candidates
                        .into_iter()
                        .map(|candidate| {
                            let name = self.standard_call_name(candidate, env)?;
                            let kind = name.strip_prefix("builtins.")?;
                            known(kind).then(|| kind.to_owned())
                        })
                        .collect::<Option<Vec<_>>>()
                })
        } else {
            None
        };
        if handlers.is_none() {
            self.cutoff("exception-handler-type-unchecked");
            self.effect_limit(
                clause,
                "exception-handler",
                "Matching custom error classes or exception groups remains unchecked.",
            );
        }
        let mut yes = Raised::new();
        let mut no = Raised::new();
        for (kind, flow) in raised {
            if let Some(handlers) = &handlers {
                if catch_all
                    || known(&kind) && handlers.iter().any(|handler| matches(&kind, handler))
                {
                    yes.insert(kind, flow);
                    continue;
                }
                if handlers.is_empty() || known(&kind) {
                    no.insert(kind, flow);
                    continue;
                }
                self.cutoff("exception-type-unchecked");
                self.effect_limit(
                    clause,
                    "exception-handler",
                    "The explicitly raised value has no established builtin error class.",
                );
            }
            yes.insert(kind.clone(), flow.clone());
            no.insert(kind, flow);
        }
        (yes, no)
    }

    pub(super) fn integer_conversion(&mut self, node: Node<'t>, env: &Environment, flow: &Flow) {
        let Some(arguments) = node.child_by_field_name("arguments") else {
            return;
        };
        let values: Vec<_> = arguments
            .named_children(&mut arguments.walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        if values.is_empty() {
            return;
        }
        if values.len() == 1
            && calls::unparenthesized(values[0]).is_some_and(|value| {
                matches!(value.kind(), "integer" | "true" | "false")
                    || value.kind() == "unary_operator"
                        && value
                            .child_by_field_name("argument")
                            .is_some_and(|argument| argument.kind() == "integer")
            })
        {
            return;
        }
        let decimal = values.len() == 1 || values.len() == 2 && self.text(values[1]) == "10";
        if !decimal || !self.plain_string(values[0], env) {
            if values.len() != 1 {
                self.cutoff("integer-conversion-type-unchecked");
            }
            self.effect_limit(node, "implicit-exception", "The scalar model propagates this conversion's value. Its exception paths need a known string and base ten; custom conversions remain unchecked.");
            return;
        }
        self.may_raise = true;
        self.capture_raised_lists(env.lists.borrow().clone());
        let raised = self.extend(flow.clone(), node, "integer-conversion-error");
        merge(
            self.exceptional_returns
                .entry("ValueError".into())
                .or_default(),
            raised,
        );
    }
}
