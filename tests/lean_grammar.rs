use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;

mod common;

#[test]
fn simplifier_locations_and_equality_transport_parse() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
}

#[test]
fn tactic_locations_preserve_later_source_declarations() {
    let source = include_str!("fixtures/lean-source-declarations.lean");
    let (_directory, index) = common::workspace(&[("Source.lean", source)]);
    for name in [
        "slice_is_maximal",
        "slice_and_remaining_partition",
        "slice_zero_budget",
        "slice_finishes_when_budget_fits",
        "slice_advances_to_available_boundary",
        "sourcePageLengths",
        "prefix_length_bounded",
        "page_preserves_rows",
        "page_respects_shared_budget",
        "page_budget_partition",
        "page_zero_budget",
    ] {
        assert!(
            index.symbols.iter().any(|symbol| symbol.name == name),
            "lost {name}"
        );
    }
    assert_eq!(index.symbols.len(), 96);
}

#[test]
fn agent_proof_forms_parse_structurally() {
    let source = r#"
def allowed (purpose : Nat) : Bool :=
  match purpose with
  | 2 | 3 | 4 => true
  | _ => false

def masked (mode before after : UInt32) : Bool :=
  mode &&& ~~~before == after ||| (mode ^^^ before) == after

theorem admitted (state : Nat) (ready : Bool)
    (values : List Nat) (member : ∀ value ∈ values, value ∈ values) : True := by
  by_cases s0 : state = 0 <;> cases ready <;> simp_all [*]

theorem branches (values : List Nat) (pair : Nat × Nat) : True := by
  induction values with
  | nil => simp
  | cons head tail ih => simp_all
  rcases pair with ⟨left, right⟩
  simp

theorem alternatives (invalid : 0 < 1 ∨ 1 < 2) : True := by
  rcases invalid with left | right
  · trivial
  · trivial
"#;

    let tree = Parsers::new()
        .parse(Language::Lean, source)
        .expect("the Lean parser loads");
    assert!(!tree.has_errors(), "agent proof forms must parse cleanly");
    let structure = tree.root().to_sexp();
    assert!(structure.contains("tactic_cases"), "{structure}");
    assert!(
        structure.contains("tactic_constructor_branch"),
        "{structure}"
    );
    assert!(structure.contains("tactic_rcases"), "{structure}");
    assert!(structure.contains("rcases_tuple_pattern"), "{structure}");
    assert!(
        structure.contains("rcases_alternative_pattern"),
        "{structure}"
    );
}
