use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Truth {
    False,
    True,
    Unknown,
}

impl Truth {
    fn join(self, other: Self) -> Self {
        if self == other {
            self
        } else {
            Self::Unknown
        }
    }

    fn invert(self) -> Self {
        match self {
            Self::False => Self::True,
            Self::True => Self::False,
            Self::Unknown => Self::Unknown,
        }
    }
}

pub(super) fn contract() -> Value {
    json!({"schema":"fr-expression-control",
        "evaluation":"left-to-right; each operand once per expression transfer.",
        "selectors":"Boolean and None literals, not, parentheses and nested selected expressions.",
        "unknowns":"retain both alternatives; no variable or call-result truth specialization.",
        "comparisons":"ordered operands; each comparison result may be true or false.",
        "effects":"join normal, sink and explicit-raise alternatives.",
        "boundary":"scalar truth and comparisons; no overloaded protocols or implicit exceptions.",
        "path_feasibility":false})
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn controlled_expression(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        self.expression_value(node, env).0
    }

    fn expression_value(&mut self, node: Node<'t>, env: &Environment) -> (Flow, Truth) {
        self.record_list_references(node, env, lists::References::default());
        if !self.normal {
            return (Flow::new(), Truth::Unknown);
        }
        if !matches!(
            node.kind(),
            "boolean_operator"
                | "conditional_expression"
                | "comparison_operator"
                | "not_operator"
                | "parenthesized_expression"
        ) {
            let flow = self.expression(node, env);
            let truth = match node.kind() {
                "true" => Truth::True,
                "false" | "none" => Truth::False,
                _ => Truth::Unknown,
            };
            return (flow, truth);
        }
        if !self.tick(node, "use") {
            return (Flow::new(), Truth::Unknown);
        }
        let children: Vec<_> = node
            .named_children(&mut node.walk())
            .filter(|child| child.kind() != "comment")
            .collect();
        let mut references = lists::References::default();
        let (flow, truth) = match node.kind() {
            "boolean_operator" if children.len() == 2 => {
                let Some(operator) = node.child_by_field_name("operator") else {
                    self.cutoff("missing-boolean-operator");
                    return (Flow::new(), Truth::Unknown);
                };
                let short = match self.text(operator) {
                    "and" => Truth::False,
                    "or" => Truth::True,
                    _ => {
                        self.cutoff("unsupported-boolean-operator");
                        return (Flow::new(), Truth::Unknown);
                    }
                };
                let (left, truth) = self.expression_value(children[0], env);
                let left_references = self.list_references(children[0], env);
                if !self.normal {
                    return (Flow::new(), Truth::Unknown);
                }
                if truth == short {
                    references = left_references;
                    (left, truth)
                } else {
                    let before = env.lists.borrow().clone();
                    let (right, right_truth) = self.expression_value(children[1], env);
                    let right_references = self.list_references(children[1], env);
                    let right_normal = self.normal;
                    if truth == Truth::Unknown {
                        env.lists.borrow_mut().join(&before);
                        references = left_references;
                        self.normal = true;
                        let mut result = left;
                        if right_normal {
                            merge(&mut result, right);
                            references.join(&right_references);
                        }
                        (
                            result,
                            if right_normal {
                                short.join(right_truth)
                            } else {
                                short
                            },
                        )
                    } else if right_normal {
                        references = right_references;
                        (right, right_truth)
                    } else {
                        (Flow::new(), Truth::Unknown)
                    }
                }
            }
            "conditional_expression" if children.len() == 3 => {
                let (_, condition) = self.expression_value(children[1], env);
                if !self.normal {
                    return (Flow::new(), Truth::Unknown);
                }
                let mut result = Flow::new();
                let mut normal = false;
                let mut truth = None;
                let before = env.lists.borrow().clone();
                let mut joined_heap = None::<lists::Lists>;
                for (index, excluded) in [(0, Truth::False), (2, Truth::True)] {
                    if condition == excluded {
                        continue;
                    }
                    *env.lists.borrow_mut() = before.clone();
                    self.normal = true;
                    let (value, alternative) = self.expression_value(children[index], env);
                    match &mut joined_heap {
                        Some(heap) => {
                            heap.join(&env.lists.borrow());
                        }
                        None => joined_heap = Some(env.lists.borrow().clone()),
                    }
                    if self.normal {
                        merge(&mut result, value);
                        let selected = self.list_references(children[index], env);
                        if normal {
                            references.join(&selected);
                        } else {
                            references = selected;
                        }
                        normal = true;
                        truth = Some(truth.map_or(alternative, |old: Truth| old.join(alternative)));
                    }
                }
                *env.lists.borrow_mut() = joined_heap.unwrap_or(before);
                self.normal = normal;
                (result, truth.unwrap_or(Truth::Unknown))
            }
            "comparison_operator" if children.len() >= 2 => {
                let mut result = Flow::new();
                for (index, child) in children.into_iter().enumerate() {
                    let (value, _) = self.expression_value(child, env);
                    if !self.normal {
                        let prior_comparison_can_finish_false = index >= 2;
                        self.normal = prior_comparison_can_finish_false;
                        return (
                            if self.normal { result } else { Flow::new() },
                            Truth::Unknown,
                        );
                    }
                    merge(&mut result, value);
                }
                (result, Truth::Unknown)
            }
            "parenthesized_expression" | "not_operator" if children.len() == 1 => {
                let (flow, truth) = self.expression_value(children[0], env);
                if node.kind() == "parenthesized_expression" {
                    references = self.list_references(children[0], env);
                }
                (
                    flow,
                    if node.kind() == "not_operator" {
                        truth.invert()
                    } else {
                        truth
                    },
                )
            }
            _ => {
                self.cutoff(format!("unsupported-expression-shape:{}", node.kind()));
                (Flow::new(), Truth::Unknown)
            }
        };
        self.record_list_references(node, env, references);
        (self.extend(flow, node, "expression"), truth)
    }
}
