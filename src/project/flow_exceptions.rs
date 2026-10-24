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
    matches!(kind, "ValueError" | "TypeError" | "Exception" | "BaseException")
}

pub(super) fn valid(value: &Value) -> bool {
    value.as_object().is_some_and(|raised| raised.iter().all(|(kind, flow)|
        (kind == "unknown" || known(kind)) && flow.as_object().is_some_and(|flows|
            flows.iter().all(|(origin, trace)| trace["origin"] == *origin && trace["occurrences"].is_array()))))
}

fn matches(kind: &str, handler: &str) -> bool {
    kind == handler || handler == "BaseException" || handler == "Exception" && kind != "BaseException"
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn raised_value(&mut self, node: Node<'t>, active: Option<usize>, env: &Environment, flow: Flow) -> Raised {
        if node.named_child_count() == 0 {
            if let Some(raised) = active.and_then(|slot| env.pending_raises.get(&slot)) {
                if raised.contains_key("unknown") {
                    self.cutoff("raise-contract-unchecked");
                }
                return raised.iter().map(|(kind, flow)| (kind.clone(), self.extend(flow.clone(), node, "reraise"))).collect();
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

    pub(super) fn match_exceptions(&mut self, clause: Node<'t>, env: &Environment, raised: Raised) -> (Raised, Raised) {
        let mut cursor = clause.walk();
        let expressions: Vec<_> = clause.named_children(&mut cursor)
            .filter(|child| !matches!(child.kind(), "block" | "comment")).collect();
        let invalid = clause.child_by_field_name("alias").is_some()
            || clause.children(&mut clause.walk()).any(|child| child.kind() == "*");
        let catch_all = !invalid && expressions.is_empty();
        let handlers = if invalid {
            None
        } else if expressions.is_empty() {
            Some(Vec::new())
        } else if expressions.len() == 1 {
            let value = calls::unparenthesized(expressions[0]);
            value.filter(|value| calls::initialization_fits(*value)).and_then(|value| {
                let candidates = if value.kind() == "tuple" {
                    value.named_children(&mut value.walk()).filter(|node| node.kind() != "comment").collect()
                } else {
                    vec![value]
                };
                candidates.into_iter().map(|candidate| {
                    let name = self.standard_call_name(candidate, env)?;
                    let kind = name.strip_prefix("builtins.")?;
                    known(kind).then(|| kind.to_owned())
                }).collect::<Option<Vec<_>>>()
            })
        } else {
            None
        };
        if handlers.is_none() {
            self.cutoff("exception-handler-type-unchecked");
            self.effect_limit(clause, "exception-handler", "Matching custom, bound or grouped errors remains unchecked.");
        }
        let mut yes = Raised::new();
        let mut no = Raised::new();
        for (kind, flow) in raised {
            if let Some(handlers) = &handlers {
                if catch_all || known(&kind) && handlers.iter().any(|handler| matches(&kind, handler)) {
                    yes.insert(kind, flow);
                    continue;
                }
                if handlers.is_empty() || known(&kind) {
                    no.insert(kind, flow);
                    continue;
                }
                self.cutoff("exception-type-unchecked");
                self.effect_limit(clause, "exception-handler", "The explicitly raised value has no established builtin error class.");
            }
            yes.insert(kind.clone(), flow.clone());
            no.insert(kind, flow);
        }
        (yes, no)
    }
}
