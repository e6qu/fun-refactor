use super::*;

enum Shape {
    Scalar(usize),
    Sequence(Vec<Shape>),
}

pub(super) fn contract() -> Value {
    json!({"schema":"fr-scalar-assignment-1",
        "evaluation":"evaluate all scalar RHS leaves left-to-right once before any target write.",
        "binding":"match literal tuple/list shapes; write local names and chained targets left-to-right.",
        "limits":"64 RHS syntax nodes, 64 target names, 64 chained targets and 16 shape levels per assignment.",
        "boundary":"no starred targets, arbitrary iterables, container-valued bindings, attribute/subscript writes, annotations or implicit unpacking exceptions.",
        "implicit_exceptions":false})
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|child| child.kind() != "comment")
        .collect()
}

fn grouped_target(node: Node<'_>) -> bool {
    node.kind() == "tuple_pattern"
        && children(node).len() == 1
        && !node
            .children(&mut node.walk())
            .any(|child| child.kind() == ",")
}

pub(super) fn target_names(node: Node<'_>) -> Option<Vec<Node<'_>>> {
    fn collect<'t>(
        node: Node<'t>,
        depth: usize,
        budget: &mut usize,
        names: &mut Vec<Node<'t>>,
    ) -> Option<()> {
        if depth > 16 || *budget == 0 {
            return None;
        }
        *budget -= 1;
        match node.kind() {
            "identifier" => names.push(node),
            "pattern_list" | "tuple_pattern" | "list_pattern" => {
                for child in children(node) {
                    collect(child, depth + 1, budget, names)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
    let mut names = Vec::new();
    collect(node, 0, &mut 128, &mut names)?;
    (names.len() <= 64).then_some(names)
}

fn shape<'t>(
    node: Node<'t>,
    depth: usize,
    budget: &mut usize,
    leaves: &mut Vec<Node<'t>>,
) -> Option<Shape> {
    if depth > 16 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    match node.kind() {
        "parenthesized_expression" => {
            let items = children(node);
            (items.len() == 1).then_some(())?;
            shape(items[0], depth + 1, budget, leaves)
        }
        "tuple" | "list" | "expression_list" => {
            let mut items = Vec::new();
            for child in children(node) {
                items.push(shape(child, depth + 1, budget, leaves)?);
            }
            Some(Shape::Sequence(items))
        }
        _ => {
            let index = leaves.len();
            leaves.push(node);
            Some(Shape::Scalar(index))
        }
    }
}

fn bindings<'t>(
    target: Node<'t>,
    value: &Shape,
    depth: usize,
    result: &mut Vec<(Node<'t>, usize)>,
) -> Option<()> {
    if depth > 16 || result.len() >= 64 {
        return None;
    }
    if grouped_target(target) {
        return bindings(children(target)[0], value, depth + 1, result);
    }
    match (target.kind(), value) {
        ("identifier", Shape::Scalar(index)) => result.push((target, *index)),
        ("pattern_list" | "tuple_pattern" | "list_pattern", Shape::Sequence(items)) => {
            let targets = children(target);
            if targets.len() != items.len() {
                return None;
            }
            for (target, item) in targets.into_iter().zip(items) {
                bindings(target, item, depth + 1, result)?;
            }
        }
        _ => return None,
    }
    Some(())
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn scalar_assignment(&mut self, node: Node<'t>, state: &mut Environment) {
        let mut targets = Vec::new();
        let mut right = node;
        while right.kind() == "assignment" {
            if targets.len() >= 64 || right.child_by_field_name("type").is_some() {
                self.cutoff("unsupported-assignment-contract");
                return;
            }
            let (Some(left), Some(value)) = (
                right.child_by_field_name("left"),
                right.child_by_field_name("right"),
            ) else {
                self.cutoff("unsupported-assignment-contract");
                return;
            };
            targets.push(left);
            right = value;
        }
        let mut leaves = Vec::new();
        let Some(shape) = shape(right, 0, &mut 64, &mut leaves) else {
            self.cutoff("assignment-shape-budget");
            return;
        };
        let mut writes = Vec::new();
        for target in targets {
            if bindings(target, &shape, 0, &mut writes).is_none() {
                self.cutoff("unsupported-assignment-shape");
                return;
            }
        }
        let mut values = Vec::new();
        for leaf in leaves {
            values.push(self.expression(leaf, state));
            if !self.normal {
                return;
            }
        }
        for (target, index) in writes {
            if !self.tick(target, "definition") {
                return;
            }
            let name = self.text(target).to_owned();
            state.values.insert(
                name.clone(),
                self.extend(values[index].clone(), target, "definition"),
            );
            state.defined.insert(name);
        }
    }
}
