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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Declaration,
    Call,
}

pub(super) fn literal_default(node: Node<'_>, source: &str) -> bool {
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

pub(super) fn literal_assignment<'s, 't>(
    node: Node<'t>,
    source: &'s str,
) -> Option<(&'s str, Node<'t>)> {
    if node.kind() != "expression_statement" || node.named_child_count() != 1 {
        return None;
    }
    let assignment = node.named_child(0)?;
    if assignment.kind() != "assignment" || assignment.child_by_field_name("type").is_some() {
        return None;
    }
    let left = assignment.child_by_field_name("left")?;
    let right = assignment.child_by_field_name("right")?;
    let name = &source[left.byte_range()];
    (left.kind() == "identifier" && name.is_ascii()).then_some((name, right))
}

fn scalar_assignment<'s>(node: Node<'_>, source: &'s str) -> Option<&'s str> {
    literal_assignment(node, source)
        .filter(|(_, value)| literal_default(*value, source))
        .map(|(name, _)| name)
}

pub(super) fn module_literal<'s>(node: Node<'_>, source: &'s str) -> Option<&'s str> {
    scalar_assignment(node, source).filter(|name| !(name.starts_with("__") && name.ends_with("__")))
}

pub(super) fn module_metadata<'s>(node: Node<'_>, source: &'s str) -> Option<&'s str> {
    scalar_assignment(node, source).filter(|name| metadata_name(name))
}

pub(super) fn module_function_doc<'s>(node: Node<'_>, source: &'s str) -> Option<&'s str> {
    if node.kind() != "expression_statement" || node.named_child_count() != 1 {
        return None;
    }
    let assignment = node.named_child(0)?;
    if assignment.kind() != "assignment" || assignment.child_by_field_name("type").is_some() {
        return None;
    }
    let left = assignment.child_by_field_name("left")?;
    let right = assignment.child_by_field_name("right")?;
    let object = left.child_by_field_name("object")?;
    let attribute = left.child_by_field_name("attribute")?;
    let name = &source[object.byte_range()];
    if left.kind() != "attribute"
        || object.kind() != "identifier"
        || !name.is_ascii()
        || &source[attribute.byte_range()] != "__doc__"
    {
        return None;
    }
    let text_literal = right.kind() == "string"
        && literal_default(right, source)
        && !source[right.byte_range()]
            .split(['\'', '"'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .chars()
            .any(|prefix| matches!(prefix, 'b' | 't'));
    (right.kind() == "none" || text_literal).then_some(name)
}

pub(super) fn module_function_alias<'s>(
    node: Node<'_>,
    source: &'s str,
) -> Option<(&'s str, &'s str)> {
    let (name, value) = literal_assignment(node, source)?;
    let target = &source[value.byte_range()];
    (value.kind() == "identifier"
        && target.is_ascii()
        && !(name.starts_with("__") && name.ends_with("__")))
    .then_some((name, target))
}

pub(super) fn module_builtin_reference<'s>(
    node: Node<'_>,
    source: &'s str,
) -> Option<(&'s str, &'s str)> {
    module_function_alias(node, source).filter(|(_, target)| {
        matches!(
            *target,
            "abs"
                | "aiter"
                | "all"
                | "anext"
                | "any"
                | "ascii"
                | "bin"
                | "bool"
                | "breakpoint"
                | "bytearray"
                | "bytes"
                | "callable"
                | "chr"
                | "classmethod"
                | "compile"
                | "complex"
                | "delattr"
                | "dict"
                | "dir"
                | "divmod"
                | "enumerate"
                | "eval"
                | "exec"
                | "filter"
                | "float"
                | "format"
                | "frozenset"
                | "getattr"
                | "globals"
                | "hasattr"
                | "hash"
                | "hex"
                | "id"
                | "input"
                | "int"
                | "isinstance"
                | "issubclass"
                | "iter"
                | "len"
                | "list"
                | "locals"
                | "map"
                | "max"
                | "memoryview"
                | "min"
                | "next"
                | "object"
                | "oct"
                | "open"
                | "ord"
                | "pow"
                | "print"
                | "property"
                | "range"
                | "repr"
                | "reversed"
                | "round"
                | "set"
                | "setattr"
                | "slice"
                | "sorted"
                | "staticmethod"
                | "str"
                | "sum"
                | "super"
                | "tuple"
                | "type"
                | "vars"
                | "zip"
                | "__import__"
        )
    })
}

pub(super) fn explicit_builtins(root: Node<'_>, source: &str) -> bool {
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.kind() == "identifier" && &source[node.byte_range()] == "__builtins__" {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return false;
            }
        }
    }
}

pub(super) fn collection_value(node: Node<'_>, source: &str, hashable: bool) -> bool {
    match node.kind() {
        "parenthesized_expression" => {
            let children: Vec<_> = node
                .named_children(&mut node.walk())
                .filter(|child| child.kind() != "comment")
                .collect();
            children.len() == 1 && collection_value(children[0], source, hashable)
        }
        "tuple" | "expression_list" => node
            .named_children(&mut node.walk())
            .filter(|child| child.kind() != "comment")
            .all(|child| collection_value(child, source, hashable)),
        "list" => {
            !hashable
                && node
                    .named_children(&mut node.walk())
                    .filter(|child| child.kind() != "comment")
                    .all(|child| collection_value(child, source, false))
        }
        "set" => {
            !hashable
                && node
                    .named_children(&mut node.walk())
                    .filter(|child| child.kind() != "comment")
                    .all(|child| collection_value(child, source, true))
        }
        "dictionary" => {
            !hashable
                && node
                    .named_children(&mut node.walk())
                    .filter(|child| child.kind() != "comment")
                    .all(|child| {
                        child.kind() == "pair"
                            && child
                                .child_by_field_name("key")
                                .is_some_and(|key| collection_value(key, source, true))
                            && child
                                .child_by_field_name("value")
                                .is_some_and(|value| collection_value(value, source, false))
                    })
        }
        _ => {
            literal_default(node, source)
                && (node.kind() != "string"
                    || !source[node.byte_range()]
                        .split(['\'', '"'])
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains('t'))
        }
    }
}

pub(super) fn initialization_fits(node: Node<'_>) -> bool {
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

pub(super) fn module_collection<'s>(node: Node<'_>, source: &'s str) -> Option<&'s str> {
    let (name, value) = literal_assignment(node, source)?;
    if name.starts_with("__") && name.ends_with("__") || !initialization_fits(value) {
        return None;
    }
    let mut root = value;
    while root.kind() == "parenthesized_expression" {
        root = root
            .named_children(&mut root.walk())
            .find(|child| child.kind() != "comment")?;
    }
    if !matches!(
        root.kind(),
        "list" | "tuple" | "expression_list" | "dictionary" | "set"
    ) {
        return None;
    }
    collection_value(value, source, false).then_some(name)
}

pub(super) fn metadata_name(name: &str) -> bool {
    matches!(
        name,
        "__version__" | "__author__" | "__license__" | "__copyright__"
    )
}

pub(super) fn unparenthesized(mut node: Node<'_>) -> Option<Node<'_>> {
    while node.kind() == "parenthesized_expression" {
        let mut cursor = node.walk();
        let mut children = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() != "comment");
        node = children.next()?;
        if children.next().is_some() {
            return None;
        }
    }
    Some(node)
}

fn builtin_type_reference(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
    let Some(node) = unparenthesized(node) else {
        return false;
    };
    let name = &source[node.byte_range()];
    node.kind() == "identifier"
        && matches!(
            name,
            "bool"
                | "bytes"
                | "complex"
                | "dict"
                | "float"
                | "frozenset"
                | "int"
                | "list"
                | "object"
                | "set"
                | "str"
                | "tuple"
                | "type"
        )
        && !shadowed(name)
}

fn builtin_type_query(node: Node<'_>, source: &str, shadowed: &impl Fn(&str) -> bool) -> bool {
    let Some(node) = unparenthesized(node) else {
        return false;
    };
    if builtin_type_reference(node, source, shadowed) {
        return true;
    }
    let Some(function) = node
        .child_by_field_name("function")
        .and_then(unparenthesized)
    else {
        return false;
    };
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return false;
    };
    let arguments: Vec<_> = arguments
        .named_children(&mut arguments.walk())
        .filter(|n| n.kind() != "comment")
        .collect();
    if node.kind() != "call"
        || function.kind() != "identifier"
        || &source[function.byte_range()] != "type"
        || shadowed("type")
        || arguments.len() != 1
    {
        return false;
    }
    let Some(value) = unparenthesized(arguments[0]) else {
        return false;
    };
    if collection_value(value, source, false) {
        return true;
    }
    let attribute = if value.kind() == "attribute" {
        value
    } else if value.kind() == "call" {
        let Some(arguments) = value.child_by_field_name("arguments") else {
            return false;
        };
        if arguments
            .named_children(&mut arguments.walk())
            .any(|n| n.kind() != "comment")
        {
            return false;
        }
        let Some(attribute) = value
            .child_by_field_name("function")
            .and_then(unparenthesized)
        else {
            return false;
        };
        attribute
    } else {
        return false;
    };
    let (Some(object), Some(member)) = (
        attribute.child_by_field_name("object"),
        attribute.child_by_field_name("attribute"),
    ) else {
        return false;
    };
    let Some(object) = unparenthesized(object) else {
        return false;
    };
    if attribute.kind() != "attribute" {
        return false;
    }
    if value.kind() == "attribute" {
        object.kind() == "identifier"
            && &source[object.byte_range()] == "object"
            && !shadowed("object")
            && &source[member.byte_range()] == "__dict__"
    } else {
        object.kind() == "dictionary"
            && object
                .named_children(&mut object.walk())
                .all(|n| n.kind() == "comment")
            && matches!(&source[member.byte_range()], "keys" | "values" | "items")
    }
}

pub(super) fn module_type_registry<'s>(
    node: Node<'_>,
    source: &'s str,
    shadowed: impl Fn(&str) -> bool,
) -> Option<&'s str> {
    let (name, value) = literal_assignment(node, source)?;
    if name.starts_with("__") && name.ends_with("__") || !initialization_fits(value) {
        return None;
    }
    let value = unparenthesized(value)?;
    if value.kind() != "dictionary" {
        return None;
    }
    let pairs: Vec<_> = value
        .named_children(&mut value.walk())
        .filter(|n| n.kind() != "comment")
        .collect();
    (!pairs.is_empty()
        && pairs.iter().all(|pair| {
            pair.kind() == "pair"
                && pair
                    .child_by_field_name("key")
                    .is_some_and(|key| builtin_type_query(key, source, &shadowed))
                && pair
                    .child_by_field_name("value")
                    .is_some_and(|value| builtin_type_reference(value, source, &shadowed))
        }))
    .then_some(name)
}

pub(super) fn module_class<'s>(
    node: Node<'_>,
    source: &'s str,
    postponed: bool,
    shadowed: impl Fn(&str) -> bool,
) -> Option<(&'s str, bool)> {
    if node.kind() != "class_definition" {
        return None;
    }
    let name_node = node.child_by_field_name("name")?;
    let body = node.child_by_field_name("body")?;
    let name = &source[name_node.byte_range()];
    if !name.is_ascii() || name.starts_with("__") && name.ends_with("__") {
        return None;
    }
    let mut constructible = true;
    for child in node.named_children(&mut node.walk()) {
        if child == name_node || child == body || child.kind() == "comment" {
            continue;
        }
        if child.kind() != "argument_list" {
            return None;
        }
        let bases: Vec<_> = child
            .named_children(&mut child.walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        if bases.is_empty() {
            continue;
        }
        if bases.len() != 1 {
            return None;
        }
        let base = unparenthesized(bases[0])?;
        let name = &source[base.byte_range()];
        if base.kind() != "identifier"
            || !matches!(name, "object" | "dict" | "Exception")
            || shadowed(name)
        {
            return None;
        }
        constructible &= name == "object";
    }
    for child in body.named_children(&mut body.walk()) {
        if matches!(child.kind(), "comment" | "pass_statement")
            || module_literal(child, source).is_some()
        {
            continue;
        }
        if child.kind() == "expression_statement"
            && child.named_child_count() == 1
            && child
                .named_child(0)
                .is_some_and(|n| n.kind() == "string" && literal_default(n, source))
        {
            continue;
        }
        if child.kind() != "function_definition" || !definition_is_inert(child, source, postponed) {
            return None;
        }
        let method = child.child_by_field_name("name")?;
        let method = &source[method.byte_range()];
        if !method.is_ascii()
            || matches!(
                method,
                "__name__"
                    | "__class__"
                    | "__module__"
                    | "__qualname__"
                    | "__doc__"
                    | "__dict__"
                    | "__weakref__"
                    | "__slots__"
                    | "__classcell__"
                    | "__annotations__"
                    | "__annotate__"
                    | "__type_params__"
                    | "__firstlineno__"
                    | "__static_attributes__"
                    | "__mro__"
                    | "__bases__"
                    | "__base__"
                    | "__abstractmethods__"
            )
        {
            return None;
        }
        constructible &= !matches!(method, "__new__" | "__init__" | "__del__");
    }
    Some((name, constructible))
}

pub(super) fn module_instance<'s>(
    node: Node<'_>,
    source: &'s str,
    classes: &BTreeSet<&str>,
) -> Option<&'s str> {
    if node.kind() != "expression_statement" || node.named_child_count() != 1 {
        return None;
    }
    let assignment = node.named_child(0)?;
    if assignment.kind() != "assignment" || assignment.child_by_field_name("type").is_some() {
        return None;
    }
    let left = assignment.child_by_field_name("left")?;
    let right = assignment.child_by_field_name("right")?;
    let name = &source[left.byte_range()];
    let function = right.child_by_field_name("function")?;
    let arguments = right.child_by_field_name("arguments")?;
    (left.kind() == "identifier"
        && name.is_ascii()
        && !(name.starts_with("__") && name.ends_with("__"))
        && right.kind() == "call"
        && function.kind() == "identifier"
        && classes.contains(&source[function.byte_range()])
        && arguments
            .named_children(&mut arguments.walk())
            .all(|n| n.kind() == "comment"))
    .then_some(name)
}

pub(super) fn module_export_list(node: Node<'_>, source: &str) -> Option<BTreeSet<String>> {
    if node.kind() != "expression_statement" || node.named_child_count() != 1 {
        return None;
    }
    let assignment = node.named_child(0)?;
    let left = assignment.child_by_field_name("left")?;
    let right = assignment.child_by_field_name("right")?;
    if assignment.kind() != "assignment"
        || assignment.child_by_field_name("type").is_some()
        || left.kind() != "identifier"
        || &source[left.byte_range()] != "__all__"
        || !matches!(right.kind(), "list" | "tuple")
    {
        return None;
    }
    let mut names = BTreeSet::new();
    for (index, child) in right
        .named_children(&mut right.walk())
        .filter(|n| n.kind() != "comment")
        .enumerate()
    {
        let text = &source[child.byte_range()];
        let quote = text.chars().next()?;
        let name = text.strip_prefix(quote)?.strip_suffix(quote)?;
        if index >= 128
            || child.kind() != "string"
            || !matches!(quote, '\'' | '"')
            || name.is_empty()
            || !name
                .chars()
                .enumerate()
                .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || i > 0 && c.is_ascii_digit())
        {
            return None;
        }
        names.insert(name.to_owned());
    }
    Some(names)
}

pub(super) fn declared_exports(root: Node<'_>, source: &str) -> Option<BTreeSet<String>> {
    let mut exports = None;
    let mut pending = Vec::new();
    for node in root.named_children(&mut root.walk()) {
        if let Some(names) = module_export_list(node, source) {
            if exports.replace(names).is_some() {
                return None;
            }
        } else {
            pending.push(node);
        }
    }
    while let Some(node) = pending.pop() {
        if node.kind() == "identifier" && &source[node.byte_range()] == "__all__" {
            return None;
        }
        let deferred = matches!(node.kind(), "function_definition" | "lambda")
            .then(|| node.child_by_field_name("body"))
            .flatten();
        pending.extend(
            node.named_children(&mut node.walk())
                .filter(|child| Some(*child) != deferred),
        );
    }
    exports
}

pub(super) fn future_annotation_nodes(root: Node<'_>, source: &str) -> BTreeSet<usize> {
    let mut result = BTreeSet::new();
    let mut first = true;
    for node in root.named_children(&mut root.walk()) {
        if node.kind() == "comment" {
            continue;
        }
        if first && node.kind() == "expression_statement" && node.named_child_count() == 1 {
            let value = node.named_child(0).unwrap();
            let prefix = source[value.byte_range()]
                .split(['\'', '"'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.kind() == "string" && literal_default(value, source) && !prefix.contains('b') {
                first = false;
                continue;
            }
        }
        first = false;
        if node.kind() != "future_import_statement" {
            break;
        }
        let names: Vec<_> = node
            .children_by_field_name("name", &mut node.walk())
            .collect();
        if names.len() != 1
            || names[0].kind() != "dotted_name"
            || &source[names[0].byte_range()] != "annotations"
        {
            return BTreeSet::new();
        }
        result.insert(node.start_byte());
    }
    result
}

pub(super) fn parameters<'t>(
    function: Node<'t>,
    source: &str,
    postponed: bool,
    scalars: &BTreeSet<String>,
) -> Option<Vec<Parameter<'t>>> {
    parameter_contract(function, source, postponed, Phase::Call, scalars)
}

pub(super) fn definition_is_inert(function: Node<'_>, source: &str, postponed: bool) -> bool {
    definition_is_inert_with_values(function, source, postponed, &BTreeSet::new())
}

pub(super) fn definition_is_inert_with_values(
    function: Node<'_>,
    source: &str,
    postponed: bool,
    values: &BTreeSet<String>,
) -> bool {
    parameter_contract(function, source, postponed, Phase::Declaration, values).is_some()
}

fn prior_value_default(node: Node<'_>, source: &str, values: &BTreeSet<String>) -> bool {
    default_name(node, source).is_some_and(|name| values.contains(name))
}

fn default_name<'s>(node: Node<'_>, source: &'s str) -> Option<&'s str> {
    let mut node = node;
    for _ in 0..128 {
        if node.kind() == "identifier" {
            return Some(&source[node.byte_range()]);
        }
        if node.kind() != "parenthesized_expression" {
            return None;
        }
        let children: Vec<_> = node
            .named_children(&mut node.walk())
            .filter(|child| child.kind() != "comment")
            .collect();
        if children.len() != 1 {
            return None;
        }
        node = children[0];
    }
    None
}

pub(super) fn captured_scalar_defaults(
    function: Node<'_>,
    source: &str,
    postponed: bool,
    scalars: &BTreeSet<String>,
) -> BTreeSet<String> {
    parameters(function, source, postponed, scalars)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|parameter| parameter.default)
        .filter_map(|node| default_name(node, source))
        .map(str::to_owned)
        .collect()
}

pub(super) fn generator_function(function: Node<'_>) -> bool {
    let mut pending: Vec<_> = function.child_by_field_name("body").into_iter().collect();
    while let Some(node) = pending.pop() {
        if node.kind() == "yield" {
            return true;
        }
        let deferred = if matches!(
            node.kind(),
            "function_definition" | "class_definition" | "lambda"
        ) {
            node.child_by_field_name("body")
        } else {
            None
        };
        pending.extend(
            node.named_children(&mut node.walk())
                .filter(|child| Some(*child) != deferred),
        );
    }
    false
}

fn parameter_contract<'t>(
    function: Node<'t>,
    source: &str,
    postponed: bool,
    phase: Phase,
    values: &BTreeSet<String>,
) -> Option<Vec<Parameter<'t>>> {
    if function.child_by_field_name("type_parameters").is_some()
        || function
            .child_by_field_name("return_type")
            .is_some_and(|node| !postponed || !postponed_annotation(node))
    {
        return None;
    }
    let list = function.child_by_field_name("parameters")?;
    let mut result: Vec<Parameter<'t>> = Vec::new();
    let mut names = BTreeSet::new();
    let mut slash = false;
    let mut star = false;
    let mut bare_star = false;
    let mut variadic_keywords = false;
    let mut optional = false;
    for mut node in list.named_children(&mut list.walk()) {
        if matches!(node.kind(), "typed_parameter" | "typed_default_parameter")
            && (!postponed || !postponed_annotation(node.child_by_field_name("type")?))
        {
            return None;
        }
        if node.kind() == "typed_parameter" {
            node = node.named_child(0)?;
        }
        match node.kind() {
            "comment" => (),
            "identifier" | "default_parameter" | "typed_default_parameter"
                if !variadic_keywords =>
            {
                let (name, default) = if node.kind() != "identifier" {
                    let name = node.child_by_field_name("name")?;
                    let value = node.child_by_field_name("value")?;
                    if name.kind() != "identifier"
                        || !(literal_default(value, source)
                            || prior_value_default(value, source, values))
                    {
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
            "keyword_separator" if !star => {
                star = true;
                bare_star = true;
            }
            "list_splat_pattern" if !star => {
                variadic_parameter(node, function, source, &mut names, phase)?;
                star = true;
            }
            "dictionary_splat_pattern" if !variadic_keywords => {
                variadic_parameter(node, function, source, &mut names, phase)?;
                variadic_keywords = true;
            }
            _ => return None,
        }
    }
    if bare_star && result.last().is_none_or(|p| p.kind != Kind::KeywordOnly) {
        return None;
    }
    Some(result)
}

fn postponed_annotation(node: Node<'_>) -> bool {
    let mut pending = vec![node];
    for _ in 0..128 {
        let Some(node) = pending.pop() else {
            return true;
        };
        if matches!(
            node.kind(),
            "await" | "yield" | "named_expression" | "splat_type" | "constrained_type"
        ) {
            return false;
        }
        pending.extend(node.named_children(&mut node.walk()));
        if pending.len() > 128 {
            return false;
        }
    }
    pending.is_empty()
}

fn variadic_parameter<'s>(
    node: Node<'_>,
    function: Node<'_>,
    source: &'s str,
    names: &mut BTreeSet<&'s str>,
    phase: Phase,
) -> Option<()> {
    let mut cursor = node.walk();
    let mut children = node.named_children(&mut cursor);
    let name = children.next()?;
    if name.kind() != "identifier" || children.next().is_some() {
        return None;
    }
    let text = &source[name.byte_range()];
    if !text.is_ascii() || !names.insert(text) {
        return None;
    }
    if phase == Phase::Declaration {
        return Some(());
    }
    let mut pending = vec![function.child_by_field_name("body")?];
    while let Some(child) = pending.pop() {
        if child.kind() == "identifier" && &source[child.byte_range()] == text {
            return None;
        }
        pending.extend(child.named_children(&mut child.walk()));
    }
    Some(())
}

pub(super) fn contract() -> Value {
    json!({"schema":"fr-call-binding",
        "parameters":"ASCII positional-only, positional-or-keyword and keyword-only slots; required or immutable scalar defaults.",
        "evaluation":"Evaluate explicit values once in source order before binding.",
        "binding":"Bind positions, then exact keyword names; substitute in declaration order.",
        "invalid":"Missing, excess, duplicate or unknown arguments leave analysis incomplete; no TypeError model.",
        "boundary":"No evaluated/mutable defaults, unpostponed annotations, type parameters, used variadics, extra arguments, unpacking, async/generator calls, external-rule keywords or non-normalized names.",
        "definitions":"Module function bodies defer with literal or known prior defaults, including variadics. Calls accept literals or captured local scalars. Annotations: checked module-local future, <=128 nodes, no await/yield/assignment/type unpacking.",
        "initialization":"Inert: literals/aliases, builtin refs/maps, exports, plain instances, prior function __doc__. Bases: none/unshadowed object/dict/Exception. Lambda/dict setup/item/update <=128 nodes; own writes only.",
        "variadics":"Omit unused ASCII variadics whose names never occur in the body. Calls bind fixed slots; annotations follow definition rules.",
        "defaults":"Omitted slots use source-free immutable scalars; explicit arguments override defaults. Definition-time effects stay unchecked.",
        "implicit_exceptions":false})
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn function_parameters<'n>(
        &self,
        function: Node<'n>,
        file: &Path,
        source: &str,
    ) -> Option<Vec<Parameter<'n>>> {
        let empty = BTreeSet::new();
        let scalars = self
            .scalar_defaults
            .get(&(file.to_path_buf(), function.start_byte()))
            .unwrap_or(&empty);
        parameters(function, source, self.annotations_postponed(file), scalars)
    }

    pub(super) fn annotations_postponed(&self, file: &Path) -> bool {
        self.modules
            .is_some_and(|modules| modules.annotations_postponed(file))
    }

    pub(super) fn parameter_nodes(&self, function: Node<'t>) -> Option<Vec<Node<'t>>> {
        if self.solver.enabled {
            self.function_parameters(function, self.file, self.source)
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
        let Some(parameters) = self.function_parameters(function, self.contexts[name].0, source)
        else {
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
        self.function_parameters(function, self.file, self.source)
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
