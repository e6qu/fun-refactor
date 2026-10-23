use super::*;

const METHODS: &[(&str, usize, usize)] = &[
    ("lower", 0, 0), ("upper", 0, 0), ("casefold", 0, 0), ("capitalize", 0, 0),
    ("swapcase", 0, 0), ("title", 0, 0), ("strip", 0, 1), ("lstrip", 0, 1),
    ("rstrip", 0, 1), ("removeprefix", 1, 1), ("removesuffix", 1, 1), ("replace", 2, 3),
];

pub(super) fn contract() -> Value {
    json!({"methods":METHODS, "receiver_nodes":128,
        "receiver":"Plain str from literals, checked str conversions, local bindings and normal helper summaries; annotations do not establish runtime types.",
        "arguments":"Positional text arguments only; stripping also accepts None, and replacement counts require literal integers.",
        "evaluation":"Receiver once, then arguments left-to-right; preserve all possible origins without mutating the receiver.",
        "boundary":"Unknown receivers, expanded arguments and unsupported signatures remain incomplete. These calls do not establish runtime exception safety."})
}

fn arguments(node: Node<'_>) -> Option<Vec<Node<'_>>> {
    let arguments = node.child_by_field_name("arguments")?;
    let values: Vec<_> = arguments.named_children(&mut arguments.walk())
        .filter(|node| node.kind() != "comment").collect();
    (!values.iter().any(|node| matches!(node.kind(), "keyword_argument" | "list_splat" | "dictionary_splat")))
        .then_some(values)
}

fn integer_literal(node: Node<'_>, source: &str) -> bool {
    let Some(node) = calls::unparenthesized(node) else { return false; };
    node.kind() == "integer" || node.kind() == "unary_operator"
        && node.child_by_field_name("operator").is_some_and(|operator| matches!(&source[operator.byte_range()], "+" | "-"))
        && node.child_by_field_name("argument").is_some_and(|value| value.kind() == "integer")
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    fn call_name(&self, function: Node<'_>) -> String {
        self.text(function).chars().filter(|character| !character.is_whitespace()).collect()
    }

    fn resolved_name(&self, name: &str) -> String {
        self.modules.map_or_else(
            || self.function_aliases.get(name).map_or_else(|| name.to_owned(), |(target, _)| target.clone()),
            |modules| modules.resolve(self.file, name))
    }

    fn callable_binding(&self, function: Node<'_>, env: &Environment) -> Option<String> {
        let name = self.call_name(function);
        let base = name.split('.').next()?;
        let resolved = self.resolved_name(&name);
        (!env.values.contains_key(base) && !self.ambiguous.contains(&resolved)
            && !self.local_bindings.last().is_some_and(|names| names.contains(base)))
            .then_some(resolved)
    }

    pub(super) fn standard_call_name(&self, function: Node<'_>, env: &Environment) -> Option<String> {
        self.callable_binding(function, env)?;
        let modules = self.modules?;
        let name = self.call_name(function);
        modules.standard_callee(self.file, &name).or_else(|| {
            let binding = modules.function_name(self.file, &name);
            (function.kind() == "identifier" && modules.standard_enabled("builtins")
                && !self.functions.contains_key(&binding) && !self.inert_bindings.contains(&binding)
                && !modules.has_binding(self.file, &name)
                && self.active.last().and_then(|name| self.functions.get(name)).is_some_and(|function| {
                    function.parent().is_some_and(|root| !calls::explicit_builtins(root, self.source))
                })).then(|| format!("builtins.{name}"))
        })
    }

    pub(super) fn plain_string(&self, node: Node<'_>, env: &Environment) -> bool {
        if !calls::initialization_fits(node) { return false; }
        let Some(node) = calls::unparenthesized(node) else { return false; };
        match node.kind() {
            "identifier" => env.defined.contains(self.text(node)) && env.plain_strings.contains(self.text(node)),
            "string" => calls::literal_default(node, self.source)
                && !self.text(node).split(['\'', '"']).next().unwrap_or("").to_ascii_lowercase().contains('b'),
            "binary_operator" => node.child_by_field_name("operator").is_some_and(|operator| self.text(operator) == "+")
                && node.child_by_field_name("left").is_some_and(|left| self.plain_string(left, env))
                && node.child_by_field_name("right").is_some_and(|right| self.plain_string(right, env)),
            "conditional_expression" => {
                let values: Vec<_> = node.named_children(&mut node.walk()).filter(|node| node.kind() != "comment").collect();
                values.len() == 3 && self.plain_string(values[0], env) && self.plain_string(values[2], env)
            }
            "call" => {
                let Some(function) = node.child_by_field_name("function") else { return false; };
                let Some(values) = arguments(node) else { return false; };
                if self.standard_call_name(function, env).as_deref() == Some("builtins.str") {
                    return values.len() <= 1;
                }
                if function.kind() == "attribute" && self.text_method_signature(node, env) {
                    return true;
                }
                self.callable_binding(function, env).is_some_and(|name| self.solver.returns_plain_string(&name))
            }
            _ => false,
        }
    }

    fn text_method_signature(&self, node: Node<'_>, env: &Environment) -> bool {
        if !self.modules.is_some_and(|modules| modules.standard_enabled("builtins")) { return false; }
        let Some(function) = node.child_by_field_name("function") else { return false; };
        if function.kind() != "attribute" { return false; }
        let (Some(object), Some(attribute), Some(values)) = (
            function.child_by_field_name("object"), function.child_by_field_name("attribute"), arguments(node)
        ) else { return false; };
        let method = self.text(attribute);
        let Some((_, minimum, maximum)) = METHODS.iter().find(|(name, _, _)| *name == method) else { return false; };
        if !(*minimum..=*maximum).contains(&values.len()) || !self.plain_string(object, env) { return false; }
        values.iter().enumerate().all(|(index, value)| {
            if method == "replace" && index == 2 { return integer_literal(*value, self.source); }
            self.plain_string(*value, env)
                || matches!(method, "strip" | "lstrip" | "rstrip")
                    && calls::unparenthesized(*value).is_some_and(|value| value.kind() == "none")
        })
    }

    pub(super) fn string_method(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        if !self.modules.is_some_and(|modules| modules.standard_enabled("builtins")) { return None; }
        let function = node.child_by_field_name("function")?;
        if function.kind() != "attribute" { return None; }
        let attribute = function.child_by_field_name("attribute")?;
        if !METHODS.iter().any(|(name, _, _)| *name == self.text(attribute)) { return None; }
        if self.callable_binding(function, env).is_some_and(|name| self.functions.contains_key(&name)) {
            return None;
        }
        let object = function.child_by_field_name("object")?;
        if !calls::initialization_fits(object) {
            self.effect_limit(node, "dynamic-call", "The receiver exceeds the 128-node check for a plain string.");
            self.cutoff("string-receiver-proof-budget");
            return Some(Flow::new());
        }
        let mut value = self.expression(object, env);
        if self.solver.enabled && !self.normal { return Some(Flow::new()); }
        if !self.plain_string(object, env) {
            self.effect_limit(node, "dynamic-call", "This check cannot establish a plain-str receiver; an object may replace this method.");
            self.cutoff("string-receiver-unchecked");
            return Some(value);
        }
        let (values, _) = self.call_arguments(node, env);
        for argument in values { merge(&mut value, argument); }
        if self.solver.enabled && !self.normal { return Some(Flow::new()); }
        if !self.text_method_signature(node, env) {
            self.effect_limit(node, "dynamic-call", "This call does not fit the checked positional string-method signature.");
            self.cutoff("string-method-signature-unchecked");
            return Some(value);
        }
        Some(self.extend(value, node, "standard-call-result"))
    }
}
