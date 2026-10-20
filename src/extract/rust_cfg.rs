use crate::model::{Symbol, SymbolId, SymbolKind};
use std::collections::BTreeMap;
use tree_sitter::Node;

fn flag(value: &str) -> bool {
    let mut characters = value.chars();
    characters.next().is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn condition(function: Node<'_>, source: &str) -> Option<(String, bool)> {
    let attribute = function.prev_named_sibling()?;
    if attribute.kind() != "attribute_item" {
        return None;
    }
    let mut previous = attribute.prev_named_sibling();
    while let Some(node) = previous {
        if node.kind() == "attribute_item" {
            return None;
        }
        if !node.kind().contains("comment") {
            break;
        }
        previous = node.prev_named_sibling();
    }
    let text = source[attribute.byte_range()].trim();
    let inner = text.strip_prefix("#[")?.strip_suffix(']')?.trim();
    let arguments = inner.strip_prefix("cfg")?.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
    if flag(arguments) {
        return Some((arguments.into(), false));
    }
    let value = arguments.strip_prefix("not")?.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
    flag(value).then(|| (value.into(), true))
}

pub(super) fn groups(root: Node<'_>, source: &str, symbols: &[Symbol]) -> Vec<Vec<SymbolId>> {
    if root.has_error() {
        return Vec::new();
    }
    let mut candidates = BTreeMap::<_, Vec<&Symbol>>::new();
    for symbol in symbols.iter().filter(|symbol| symbol.kind == SymbolKind::Function) {
        candidates.entry((&symbol.name, symbol.scope, symbol.container)).or_default().push(symbol);
    }
    candidates.into_values().filter_map(|peers| {
        if peers.len() != 2 {
            return None;
        }
        let conditions: Option<Vec<_>> = peers.iter().map(|peer| {
            let mut node = root.descendant_for_byte_range(peer.name_span.start, peer.name_span.end)?;
            while node.kind() != "function_item" {
                node = node.parent()?;
            }
            condition(node, source)
        }).collect();
        let conditions = conditions?;
        (conditions[0].0 == conditions[1].0 && conditions[0].1 != conditions[1].1)
            .then(|| peers.into_iter().map(|peer| peer.id).collect())
    }).collect()
}
