use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;

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
}
