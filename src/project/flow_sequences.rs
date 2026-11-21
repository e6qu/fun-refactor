use super::*;
use lists::Element;

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|child| child.kind() != "comment")
        .collect()
}

fn starred(node: Node<'_>) -> bool {
    matches!(node.kind(), "list_splat_pattern" | "list_splat")
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn sequence_items(
        &mut self,
        node: Node<'t>,
        value: &Element,
        env: &Environment,
    ) -> Option<Vec<Element>> {
        if value.references.ids.is_empty() && value.facts.noniterable {
            self.sequence_error(node, env, "TypeError");
            return None;
        }
        self.exact_list(&value.references, env)
            .map(|value| value.items)
    }

    pub(super) fn unpacked_assignment(
        &mut self,
        targets: &[Node<'t>],
        right: Node<'t>,
        state: &mut Environment,
    ) {
        let names: Option<Vec<_>> = targets
            .iter()
            .map(|target| assignments::target_names(*target))
            .collect();
        if names.is_none_or(|names| names.iter().map(Vec::len).sum::<usize>() > 64) {
            self.cutoff("unsupported-assignment-shape");
            return;
        }
        let flow = self.expression(right, state);
        if !self.normal {
            return;
        }
        let value = self.list_element(right, flow, state);
        let mut budget = 128;
        for target in targets {
            self.unpack_target(*target, &value, state, 0, &mut budget);
            if !self.normal {
                break;
            }
        }
    }

    fn unpack_target(
        &mut self,
        target: Node<'t>,
        value: &Element,
        state: &mut Environment,
        depth: usize,
        budget: &mut usize,
    ) {
        if depth > 16 || *budget == 0 {
            self.cutoff("assignment-shape-budget");
            return;
        }
        *budget -= 1;
        if target.kind() == "identifier" {
            self.bind_element(target, value, state);
            return;
        }
        let targets = children(target);
        if target.kind() == "tuple_pattern"
            && targets.len() == 1
            && !target
                .children(&mut target.walk())
                .any(|child| child.kind() == ",")
        {
            self.unpack_target(targets[0], value, state, depth + 1, budget);
            return;
        }
        let stars: Vec<_> = targets
            .iter()
            .enumerate()
            .filter_map(|(i, node)| starred(*node).then_some(i))
            .collect();
        if stars.len() > 1 {
            self.cutoff("unsupported-assignment-shape");
            return;
        }
        let Some(items) = self.sequence_items(target, value, state) else {
            return;
        };
        if stars.is_empty() && items.len() != targets.len()
            || !stars.is_empty() && items.len() + 1 < targets.len()
        {
            self.sequence_error(target, state, "ValueError");
            return;
        }
        let star = stars.first().copied();
        let mut values = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            let element = if Some(index) == star {
                let end = items.len() - (targets.len() - index - 1);
                let flow = self.allocate_list(*target, items[index..end].to_vec(), state);
                self.list_element(*target, flow, state)
            } else {
                let slot = if star.is_some_and(|star| index > star) {
                    items.len() - (targets.len() - index)
                } else {
                    index
                };
                items[slot].clone()
            };
            values.push(element);
        }
        for (target, value) in targets.into_iter().zip(values) {
            let target = if starred(target) {
                children(target)[0]
            } else {
                target
            };
            self.unpack_target(target, &value, state, depth + 1, budget);
            if !self.normal {
                return;
            }
        }
    }

    pub(super) fn expanded_binding_error(
        &mut self,
        function: Node<'t>,
        name: &str,
        names: &[Option<String>],
        call: Node<'t>,
        env: &Environment,
    ) -> bool {
        let (file, source) = self.contexts[name];
        let Some(parameters) = self.function_parameters(function, file, source) else {
            return false;
        };
        if function
            .child_by_field_name("parameters")
            .is_some_and(|node| {
                children(node).iter().any(|node| {
                    matches!(
                        node.kind(),
                        "list_splat_pattern" | "dictionary_splat_pattern"
                    )
                })
            })
        {
            return false;
        }
        let valid = calls::argument_slots(&parameters, source, names).is_some_and(|slots| {
            slots
                .iter()
                .zip(&parameters)
                .all(|(slot, parameter)| slot.is_some() || parameter.default.is_some())
        });
        if !valid {
            self.sequence_error(call, env, "TypeError");
        }
        !valid
    }
}
