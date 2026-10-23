//! Locations where the scalar model leaves runtime effects unchecked.
use super::*;

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn effect_limit(&mut self, node: Node<'t>, kind: &str, reason: &str) {
        let occurrence = self.occurrence(node, "unchecked-effect");
        let key = format!("{}:{kind}", occurrence.id);
        self.effect_limits.entry(key).or_insert_with(|| {
            json!({"kind":kind,"reason":reason,"occurrence":occurrence})
        });
    }

    pub(super) fn expression_effect_limit(&mut self, node: Node<'t>) {
        match node.kind() {
            "binary_operator"
                if node.child_by_field_name("operator").is_some_and(|operator| {
                    matches!(self.text(operator), "/" | "//" | "%" | "**" | "<<" | ">>")
                }) =>
            {
                self.effect_limit(node, "implicit-exception",
                    "The scalar model follows values through this operation; it does not establish that arithmetic finishes without an exception.");
            }
            "subscript" | "attribute" => self.effect_limit(node, "shared-read",
                "Reading an item or attribute can observe a shared object or raise; this scalar analysis does not model that operation."),
            "list" | "dictionary" | "set" => self.effect_limit(node, "mutable-value",
                "This creates a mutable object. The scalar analysis does not track its identity or changes through other names."),
            _ => (),
        }
    }
}
