use super::*;

#[derive(Default)]
pub(super) struct Allocations<'s> {
    dictionaries: BTreeSet<&'s str>,
}

pub(super) enum Statement<'s> {
    Declaration(&'s str),
    Population,
}

fn bounded(node: Node<'_>) -> bool {
    let mut pending = vec![node];
    let mut count = 0;
    while let Some(node) = pending.pop() {
        count += 1;
        if count > 128 {
            return false;
        }
        for child in node.named_children(&mut node.walk()) {
            if count + pending.len() >= 128 {
                return false;
            }
            pending.push(child);
        }
    }
    true
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|node| node.kind() != "comment")
        .collect()
}

fn dictionary(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
    let Some(node) = calls::unparenthesized(node) else {
        return false;
    };
    if node.kind() == "dictionary" {
        return children(node).iter().all(|pair| {
            pair.kind() == "pair"
                && pair
                    .child_by_field_name("key")
                    .is_some_and(|key| calls::collection_value(key, source, true))
                && pair
                    .child_by_field_name("value")
                    .is_some_and(|node| value(node, source, shadowed))
        });
    }
    let Some(function) = node
        .child_by_field_name("function")
        .and_then(calls::unparenthesized)
    else {
        return false;
    };
    if node.kind() != "call"
        || function.kind() != "identifier"
        || &source[function.byte_range()] != "dict"
        || shadowed("dict")
    {
        return false;
    }
    node.child_by_field_name("arguments")
        .is_some_and(|args| arguments(args, source, shadowed))
}

fn arguments(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
    let mut keywords = BTreeSet::new();
    children(node).iter().enumerate().all(|(index, node)| {
        if node.kind() != "keyword_argument" {
            return index == 0 && dictionary(*node, source, shadowed);
        }
        let Some(name) = node.child_by_field_name("name") else {
            return false;
        };
        name.kind() == "identifier"
            && source[name.byte_range()].is_ascii()
            && keywords.insert(&source[name.byte_range()])
            && node
                .child_by_field_name("value")
                .is_some_and(|node| value(node, source, shadowed))
    })
}

fn value(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
    let Some(node) = calls::unparenthesized(node) else {
        return false;
    };
    if calls::collection_value(node, source, false) {
        return true;
    }
    match node.kind() {
        "lambda" => {
            node.child_by_field_name("parameters").is_none()
                || calls::definition_is_inert(node, source, false)
        }
        "list" | "tuple" | "expression_list" => children(node)
            .iter()
            .all(|node| value(*node, source, shadowed)),
        "dictionary" | "call" => dictionary(node, source, shadowed),
        _ => false,
    }
}

impl<'s> Allocations<'s> {
    pub fn statement(
        &mut self,
        node: Node<'_>,
        source: &'s str,
        shadowed: impl Fn(&str) -> bool,
    ) -> Option<Statement<'s>> {
        if let Some((name, right)) = calls::literal_assignment(node, source) {
            self.dictionaries.remove(name);
            let ordinary = !(name.starts_with("__") && name.ends_with("__"));
            if ordinary && bounded(right) && value(right, source, &shadowed) {
                if !shadowed(name) && dictionary(right, source, &shadowed) {
                    self.dictionaries.insert(name);
                }
                if calls::module_literal(node, source).is_none()
                    && calls::module_collection(node, source).is_none()
                {
                    return Some(Statement::Declaration(name));
                }
                return None;
            }
        }
        if bounded(node) && self.population(node, source, &shadowed) {
            return Some(Statement::Population);
        }
        if matches!(node.kind(), "comment" | "pass_statement")
            || node.kind() == "function_definition"
                && calls::definition_is_inert(node, source, false)
            || calls::module_metadata(node, source).is_some()
            || calls::module_export_list(node, source).is_some()
        {
            return None;
        }
        self.dictionaries.clear();
        None
    }

    fn population(&self, node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
        if node.kind() != "expression_statement" || node.named_child_count() != 1 {
            return false;
        }
        let Some(expression) = node.named_child(0) else {
            return false;
        };
        if expression.kind() == "assignment" && expression.child_by_field_name("type").is_none() {
            let Some(left) = expression.child_by_field_name("left") else {
                return false;
            };
            let Some(object) = left
                .child_by_field_name("value")
                .and_then(calls::unparenthesized)
            else {
                return false;
            };
            return left.kind() == "subscript"
                && object.kind() == "identifier"
                && self.dictionaries.contains(&source[object.byte_range()])
                && children(left).len() == 2
                && left
                    .child_by_field_name("subscript")
                    .is_some_and(|key| calls::collection_value(key, source, true))
                && expression
                    .child_by_field_name("right")
                    .is_some_and(|node| value(node, source, shadowed));
        }
        let Some(function) = expression
            .child_by_field_name("function")
            .and_then(calls::unparenthesized)
        else {
            return false;
        };
        let Some(object) = function
            .child_by_field_name("object")
            .and_then(calls::unparenthesized)
        else {
            return false;
        };
        expression.kind() == "call"
            && function.kind() == "attribute"
            && object.kind() == "identifier"
            && self.dictionaries.contains(&source[object.byte_range()])
            && function
                .child_by_field_name("attribute")
                .is_some_and(|name| &source[name.byte_range()] == "update")
            && expression
                .child_by_field_name("arguments")
                .is_some_and(|args| arguments(args, source, shadowed))
    }
}
