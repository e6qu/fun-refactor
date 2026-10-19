use super::*;

pub(super) fn unchecked_reason(node: Node<'_>) -> &'static str {
    match node.kind() {
        "class_definition" => "class-definition",
        "decorated_definition" => "decorated-definition",
        "import_statement" | "import_from_statement" | "future_import_statement" => {
            "startup-import"
        }
        "if_statement" | "for_statement" | "while_statement" | "try_statement"
        | "with_statement" | "match_statement" => "startup-control-flow",
        "expression_statement" if node.named_child_count() == 1 => {
            let expression = node.named_child(0).unwrap();
            let value = expression
                .child_by_field_name("right")
                .unwrap_or(expression);
            let bounded = if expression.kind() == "assignment"
                && expression
                    .child_by_field_name("left")
                    .is_some_and(|n| n.kind() == "identifier")
            {
                value
            } else {
                node
            };
            if !calls::initialization_fits(bounded) {
                "initializer-node-budget"
            } else if matches!(expression.kind(), "assignment" | "augmented_assignment")
                && expression
                    .child_by_field_name("left")
                    .is_some_and(|n| matches!(n.kind(), "attribute" | "subscript"))
            {
                "startup-write"
            } else if calls::unparenthesized(value).is_some_and(|n| n.kind() == "call") {
                "startup-call"
            } else if expression.kind() == "assignment" {
                "startup-assignment"
            } else {
                "module-statement"
            }
        }
        "delete_statement" => "startup-write",
        _ => "module-statement",
    }
}

#[derive(Default)]
pub(super) struct Allocations<'s, 't> {
    dictionaries: BTreeSet<&'s str>,
    registrations: BTreeMap<&'s str, Node<'t>>,
    known_dictionaries: BTreeMap<&'s str, Node<'t>>,
}

pub(super) enum Statement<'s> {
    Declaration(&'s str),
    Population,
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|node| node.kind() != "comment")
        .collect()
}

fn dictionary(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool, locals: &BTreeMap<&str, Node<'_>>) -> bool {
    let Some(node) = calls::unparenthesized(node) else {
        return false;
    };
    if node.kind() == "dictionary" {
        return children(node).iter().all(|pair| {
            pair.kind() == "pair"
                && pair
                    .child_by_field_name("key")
                    .is_some_and(|key| key_value(key, source, locals))
                && pair
                    .child_by_field_name("value")
                    .is_some_and(|node| value(node, source, shadowed, locals))
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
        .is_some_and(|args| arguments(args, source, shadowed, locals))
}

fn arguments(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool, locals: &BTreeMap<&str, Node<'_>>) -> bool {
    let mut keywords = BTreeSet::new();
    children(node).iter().enumerate().all(|(index, node)| {
        if node.kind() != "keyword_argument" {
            return index == 0 && dictionary(*node, source, shadowed, locals);
        }
        let Some(name) = node.child_by_field_name("name") else {
            return false;
        };
        name.kind() == "identifier"
            && source[name.byte_range()].is_ascii()
            && keywords.insert(&source[name.byte_range()])
            && node
                .child_by_field_name("value")
                .is_some_and(|node| value(node, source, shadowed, locals))
    })
}

fn value(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool, locals: &BTreeMap<&str, Node<'_>>) -> bool {
    let Some(node) = calls::unparenthesized(node) else {
        return false;
    };
    if node.kind() == "identifier" {
        if let Some(value) = locals.get(&source[node.byte_range()]) {
            return value_is_literal(*value, source, shadowed);
        }
    }
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
            .all(|node| value(*node, source, shadowed, locals)),
        "dictionary" | "call" => dictionary(node, source, shadowed, locals),
        _ => false,
    }
}

fn value_is_literal(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
    value(node, source, shadowed, &BTreeMap::new())
}

fn key_value(node: Node<'_>, source: &str, locals: &BTreeMap<&str, Node<'_>>) -> bool {
    let Some(node) = calls::unparenthesized(node) else { return false; };
    let resolved = if node.kind() == "identifier" {
        locals.get(&source[node.byte_range()]).copied().unwrap_or(node)
    } else { node };
    calls::collection_value(resolved, source, true)
}

struct Registration<'s, 't> {
    locals: BTreeMap<&'s str, Node<'t>>,
    names: BTreeSet<&'s str>,
    reading: BTreeSet<&'s str>,
    written: BTreeSet<&'s str>,
    remaining: usize,
}

fn node_count(node: Node<'_>) -> usize {
    1 + node.named_children(&mut node.walk()).map(node_count).sum::<usize>()
}

impl<'s, 't> Allocations<'s, 't> {
    pub fn statement(
        &mut self,
        node: Node<'t>,
        source: &'s str,
        shadowed: impl Fn(&str) -> bool,
    ) -> Option<Statement<'s>> {
        if let Some((name, right)) = calls::literal_assignment(node, source) {
            self.dictionaries.remove(name);
            self.registrations.remove(name);
            self.known_dictionaries.remove(name);
            let ordinary = !(name.starts_with("__") && name.ends_with("__"));
            if ordinary && calls::initialization_fits(right) && value_is_literal(right, source, &shadowed) {
                if !shadowed(name) && dictionary(right, source, &shadowed, &BTreeMap::new()) {
                    self.dictionaries.insert(name);
                    self.known_dictionaries.insert(name, right);
                }
                if calls::module_literal(node, source).is_none()
                    && calls::module_collection(node, source).is_none()
                {
                    return Some(Statement::Declaration(name));
                }
                return None;
            }
        }
        if calls::initialization_fits(node) {
            if let Some(name) = self.population(node, source, &shadowed, &BTreeMap::new()) {
                self.known_dictionaries.remove(name);
                return Some(Statement::Population);
            }
            if let Some(written) = self.registration_call(node, source, &shadowed) {
                for name in written { self.known_dictionaries.remove(name); }
                return Some(Statement::Population);
            }
        }
        if node.kind() == "function_definition" && calls::definition_is_inert(node, source, false) {
            if let Some(name) = node.child_by_field_name("name") {
                let name = &source[name.byte_range()];
                self.dictionaries.remove(name);
                self.registrations.remove(name);
            self.known_dictionaries.remove(name);
                if name.is_ascii()
                    && !(name.starts_with("__") && name.ends_with("__"))
                    && !shadowed(name)
                    && calls::initialization_fits(node)
                    && node
                        .child_by_field_name("parameters")
                        .is_some_and(|parameters| children(parameters).is_empty())
                    && !node
                        .children(&mut node.walk())
                        .any(|child| child.kind() == "async")
                {
                    self.registrations.insert(name, node);
                }
            }
            return None;
        }
        if matches!(node.kind(), "comment" | "pass_statement")
            || calls::module_metadata(node, source).is_some()
            || calls::module_export_list(node, source).is_some()
        {
            return None;
        }
        self.dictionaries.clear();
        self.registrations.clear();
        self.known_dictionaries.clear();
        None
    }

    fn registration_call(
        &self, node: Node<'t>, source: &'s str, shadowed: &impl Fn(&str) -> bool,
    ) -> Option<BTreeSet<&'s str>> {
        if node.kind() != "expression_statement" || node.named_child_count() != 1 { return None; }
        let call = calls::unparenthesized(node.named_child(0)?)?;
        let function = calls::unparenthesized(call.child_by_field_name("function")?)?;
        if call.kind() != "call" || function.kind() != "identifier"
            || !children(call.child_by_field_name("arguments")?).is_empty() { return None; }
        let body = self.registrations.get(&source[function.byte_range()])?
            .child_by_field_name("body")?;
        let mut state = Registration { locals: BTreeMap::new(), names: BTreeSet::new(),
            reading: BTreeSet::new(), written: BTreeSet::new(), remaining: 128 };
        // Python treats every loop target as a local throughout the function.
        if !self.loop_names(body, source, shadowed, &mut state.names) { return None; }
        if self.block(body, source, shadowed, &mut state, true) && !state.written.is_empty() {
            Some(state.written)
        } else { None }
    }

    fn loop_names(&self, node: Node<'t>, source: &'s str,
        shadowed: &impl Fn(&str) -> bool, names: &mut BTreeSet<&'s str>) -> bool {
        if node.kind() == "for_statement" {
            let Some(left) = node.child_by_field_name("left") else { return false; };
            let targets = if left.kind() == "identifier" { vec![left] } else { children(left) };
            for target in targets {
                let name = &source[target.byte_range()];
                if target.kind() != "identifier" || !name.is_ascii()
                    || name.starts_with("__") && name.ends_with("__") || shadowed(name)
                    || self.dictionaries.contains(name) || !names.insert(name) { return false; }
            }
        }
        children(node).into_iter().all(|child| self.loop_names(child, source, shadowed, names))
    }

    fn block(&self, body: Node<'t>, source: &'s str, shadowed: &impl Fn(&str) -> bool,
        state: &mut Registration<'s, 't>, returns: bool) -> bool {
        let statements = children(body);
        for (index, statement) in statements.iter().copied().enumerate() {
            if statement.kind() == "for_statement" {
                if !self.iteration(statement, source, shadowed, state) { return false; }
                continue;
            }
            let Some(remaining) = state.remaining.checked_sub(node_count(statement)) else { return false; };
            state.remaining = remaining;
            if statement.kind() == "pass_statement" { continue; }
            if returns && statement.kind() == "return_statement" && index + 1 == statements.len()
                && children(statement).iter().all(|value| value.kind() == "none") { continue; }
            let Some(name) = self.population(statement, source, &|name| shadowed(name) || state.names.contains(name), &state.locals) else { return false; };
            if state.reading.contains(name) { return false; }
            state.written.insert(name);
        }
        true
    }

    fn iteration(&self, node: Node<'t>, source: &'s str, shadowed: &impl Fn(&str) -> bool,
        state: &mut Registration<'s, 't>) -> bool {
        if node.child_by_field_name("alternative").is_some()
            || node.children(&mut node.walk()).any(|child| child.kind() == "async") { return false; }
        let (Some(left), Some(right), Some(body)) = (node.child_by_field_name("left"),
            node.child_by_field_name("right").and_then(calls::unparenthesized), node.child_by_field_name("body"))
            else { return false; };
        let Some(remaining) = state.remaining.checked_sub(node_count(node) - node_count(body)) else { return false; };
        state.remaining = remaining;
        let targets = if left.kind() == "identifier" { vec![left] } else { children(left) };
        let names: Vec<_> = targets.iter().map(|node| &source[node.byte_range()]).collect();
        let mut reading = None;
        let entries: Vec<Vec<Node<'t>>> = if matches!(right.kind(), "tuple" | "list" | "expression_list")
            && left.kind() == "identifier" {
            let values = children(right);
            if !values.iter().all(|node| value_is_literal(*node, source, &|name| shadowed(name) || state.names.contains(name))) { return false; }
            values.into_iter().map(|node| vec![node]).collect()
        } else {
            if right.kind() != "call" || targets.len() != 2
                || !matches!(left.kind(), "pattern_list" | "tuple_pattern")
                || !right.child_by_field_name("arguments").is_some_and(|args| children(args).is_empty()) { return false; }
            let Some(function) = right.child_by_field_name("function") else { return false; };
            let Some(object) = function.child_by_field_name("object") else { return false; };
            let name = &source[object.byte_range()];
            if function.kind() != "attribute" || object.kind() != "identifier"
                || !function.child_by_field_name("attribute").is_some_and(|attr| &source[attr.byte_range()] == "items")
                || state.written.contains(name) || state.reading.contains(name) { return false; }
            let Some(dictionary) = self.known_dictionaries.get(name).copied().and_then(calls::unparenthesized)
                .filter(|node| node.kind() == "dictionary") else { return false; };
            reading = Some(name);
            children(dictionary).into_iter().map(|pair| {
                vec![pair.child_by_field_name("key").unwrap(), pair.child_by_field_name("value").unwrap()]
            }).collect()
        };
        if entries.is_empty() { return false; }
        if let Some(name) = reading { state.reading.insert(name); }
        for entry in entries {
            for (name, value) in names.iter().zip(entry) { state.locals.insert(name, value); }
            if !self.block(body, source, shadowed, state, false) { return false; }
        }
        for name in names { state.locals.remove(name); }
        if let Some(name) = reading { state.reading.remove(name); }
        true
    }

    fn population(&self, node: Node<'_>, source: &'s str, shadowed: &impl Fn(&str) -> bool, locals: &BTreeMap<&str, Node<'_>>) -> Option<&'s str> {
        if node.kind() != "expression_statement" || node.named_child_count() != 1 {
            return None;
        }
        let Some(expression) = node.named_child(0) else {
            return None;
        };
        if expression.kind() == "assignment" && expression.child_by_field_name("type").is_none() {
            let Some(left) = expression.child_by_field_name("left") else {
                return None;
            };
            let Some(object) = left
                .child_by_field_name("value")
                .and_then(calls::unparenthesized)
            else {
                return None;
            };
            return (left.kind() == "subscript"
                && object.kind() == "identifier"
                && self.dictionaries.contains(&source[object.byte_range()])
                && children(left).len() == 2
                && left
                    .child_by_field_name("subscript")
                    .is_some_and(|key| key_value(key, source, locals))
                && expression
                    .child_by_field_name("right")
                    .is_some_and(|node| value(node, source, shadowed, locals)))
                .then_some(&source[object.byte_range()]);
        }
        let Some(function) = expression
            .child_by_field_name("function")
            .and_then(calls::unparenthesized)
        else {
            return None;
        };
        let Some(object) = function
            .child_by_field_name("object")
            .and_then(calls::unparenthesized)
        else {
            return None;
        };
        (expression.kind() == "call"
            && function.kind() == "attribute"
            && object.kind() == "identifier"
            && self.dictionaries.contains(&source[object.byte_range()])
            && function
                .child_by_field_name("attribute")
                .is_some_and(|name| &source[name.byte_range()] == "update")
            && expression
                .child_by_field_name("arguments")
                .is_some_and(|args| arguments(args, source, shadowed, locals)))
            .then_some(&source[object.byte_range()])
    }
}
