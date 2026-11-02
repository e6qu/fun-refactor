use super::{ProofClaim, ProofClaimPart};
use crate::{lang::Language, parse::Parsers};
use anyhow::{bail, Context, Result};
use tree_sitter::Node;

fn ungroup(mut node: Node<'_>) -> Result<Node<'_>> {
    for _ in 0..=64 {
        if node.kind() != "parenthesized"
            || node.named_child_count() != 1
            || node.child_by_field_name("type").is_some()
        {
            return Ok(node);
        }
        node = node
            .named_child(0)
            .context("claim parentheses lack a term")?;
    }
    bail!("proof claim exceeds 64 nested parentheses.")
}

fn conjuncts<'a>(node: Node<'a>, source: &str) -> Result<Vec<Node<'a>>> {
    let mut pending = vec![(node, 0)];
    let mut parts = Vec::new();
    while let Some((node, depth)) = pending.pop() {
        if depth > 64 || pending.len() + parts.len() >= 64 {
            bail!("proof claim exceeds 64 parts or conjunction levels.");
        }
        let node = ungroup(node)?;
        if node.kind() == "binary_expression"
            && node
                .child_by_field_name("operator")
                .is_some_and(|operator| &source[operator.byte_range()] == "∧")
        {
            for field in ["right", "left"] {
                pending.push((
                    node.child_by_field_name(field)
                        .context("claim conjunction lacks an operand")?,
                    depth + 1,
                ));
            }
        } else {
            parts.push(node);
        }
    }
    Ok(parts)
}

pub(super) fn outline(theorem: &str) -> Result<ProofClaim> {
    if theorem.len() > 65_536 {
        bail!("proof claim header exceeds 65536 UTF-8 bytes.");
    }
    let source = format!("{theorem} := by\n  sorry\n");
    let parsed = Parsers::new().parse(Language::Lean, &source)?;
    if parsed.has_errors() {
        bail!("fr cannot read the proof claim's syntax; no files changed.");
    }
    let mut cursor = parsed.root().walk();
    let definition = parsed
        .root()
        .named_children(&mut cursor)
        .find(|node| node.kind() == "definition")
        .context("proof claim has no theorem declaration")?;
    let mut parameters = Vec::new();
    if let Some(binders) = definition.child_by_field_name("binders") {
        let mut cursor = binders.walk();
        for binder in binders.named_children(&mut cursor) {
            if parameters.len() == 64 {
                bail!("proof claim exceeds 64 parameter groups.");
            }
            parameters.push(source[binder.byte_range()].to_string());
        }
    }
    let proposition = definition
        .child_by_field_name("type")
        .context("proof claim has no declared type")?;
    let mut parts = Vec::new();
    let mut fields = 0;
    for node in conjuncts(proposition, &source)? {
        let mut conclusion = node;
        let mut assumptions = Vec::new();
        loop {
            conclusion = ungroup(conclusion)?;
            if conclusion.kind() != "arrow" {
                break;
            }
            let premise = conclusion
                .child_by_field_name("domain")
                .context("claim implication lacks a premise")?;
            for assumption in conjuncts(premise, &source)? {
                assumptions.push(source[assumption.byte_range()].to_string());
            }
            if assumptions.len() > 64 {
                bail!("proof claim exceeds 64 assumptions.");
            }
            conclusion = conclusion
                .child_by_field_name("codomain")
                .context("claim implication lacks a conclusion")?;
        }
        let conclusions = conjuncts(conclusion, &source)?
            .into_iter()
            .map(|node| source[node.byte_range()].to_string())
            .collect::<Vec<_>>();
        fields += assumptions.len() + conclusions.len();
        if fields > 64 {
            bail!("proof claim exceeds 64 assumption and conclusion fields.");
        }
        parts.push(ProofClaimPart {
            assumptions,
            conclusions,
        });
    }
    Ok(ProofClaim { parameters, parts })
}

#[cfg(test)]
mod tests {
    use super::outline;

    #[test]
    fn claims_keep_alternatives_and_separate_each_input_bound() {
        let claim =
            outline(include_str!("../../tests/fixtures/lean-claim-bounds.txt").trim()).unwrap();
        assert_eq!(claim.parameters, ["(items size : Int)"]);
        assert_eq!(claim.parts.len(), 3);
        assert_eq!(claim.parts[0].assumptions, ["items < 0 ∨ size ≤ 0"]);
        assert_eq!(
            claim.parts[1].assumptions,
            ["0 ≤ items", "items ≤ 99", "1 ≤ size", "size ≤ 99"]
        );
        assert_eq!(claim.parts[1].conclusions, ["0 ≤ items", "items ≤ 99"]);
        assert!(claim.parts[2].assumptions.is_empty());
        assert_eq!(claim.parts[2].conclusions, ["11 = 11"]);
    }

    #[test]
    fn claims_preserve_implication_scope_and_bound_variables() {
        let claim =
            outline(include_str!("../../tests/fixtures/lean-claim-scopes.txt").trim()).unwrap();
        assert_eq!(claim.parameters, ["{p q r : Prop}", "(h : p ∧ q)"]);
        assert_eq!(claim.parts.len(), 4);
        assert_eq!(claim.parts[0].assumptions, ["p → q"]);
        assert_eq!(claim.parts[1].assumptions, ["p", "q"]);
        assert_eq!(claim.parts[2].conclusions, ["∀ x : Nat, x = x ∧ x ≤ x"]);
        assert_eq!(claim.parts[3].conclusions, ["¬ (p ∧ q)"]);
    }

    #[test]
    fn claims_refuse_invalid_and_excessive_syntax() {
        assert!(outline("theorem broken :").is_err());
        let at_limit = format!("theorem nested : {}True{}", "(".repeat(64), ")".repeat(64));
        assert_eq!(outline(&at_limit).unwrap().parts[0].conclusions, ["True"]);
        let nested = format!("theorem nested : {}True{}", "(".repeat(70), ")".repeat(70));
        assert!(outline(&nested).unwrap_err().to_string().contains("64"));
        let many = format!("theorem many : {}True", "True ∧ ".repeat(65));
        assert!(outline(&many).is_err());
    }
}
