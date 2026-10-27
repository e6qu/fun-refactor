use fun_refactor::lang::Language;
use fun_refactor::model::SymbolKind;
use fun_refactor::parse::Parsers;
use std::collections::BTreeSet;

mod common;

#[test]
fn simplifier_locations_and_equality_transport_parse() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    assert_eq!(tree.root().to_sexp().matches("(for_in ").count(), 2);
    assert_eq!(tree.root().to_sexp().matches("(do_while ").count(), 2);
    assert_eq!(tree.root().to_sexp().matches("(do_unless ").count(), 1);
}

#[test]
fn fields_and_subscripts_belong_to_the_function_argument() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    let mut cursor = tree.root().walk();
    let mut checked = 0;
    for definition in tree.root().named_children(&mut cursor) {
        if definition.kind() != "definition" {
            continue;
        }
        let name = definition.child_by_field_name("name").unwrap();
        let body = definition.child_by_field_name("body").unwrap();
        match &source[name.byte_range()] {
            "projectionArgument" | "subscriptArgument" => {
                checked += 1;
                assert_eq!(body.kind(), "application");
                let head = body.child_by_field_name("name").unwrap();
                assert_eq!(&source[head.byte_range()], "f");
                let argument = body.child_by_field_name("arguments").unwrap();
                assert!(matches!(argument.kind(), "projection" | "subscript"));
            }
            "resultProjection" => {
                checked += 1;
                assert_eq!(body.kind(), "projection");
                let term = body.child_by_field_name("term").unwrap();
                assert_eq!(term.kind(), "parenthesized");
                assert_eq!(&source[term.byte_range()], "(f n)");
            }
            "tailLambda" | "tailDo" => {
                checked += 1;
                assert_eq!(body.kind(), "application");
                let argument = body.child_by_field_name("arguments").unwrap();
                assert!(matches!(argument.kind(), "fun" | "do"));
            }
            "parenthesizedProof" => {
                checked += 1;
                assert_eq!(body.kind(), "application");
                let argument = body.child_by_field_name("arguments").unwrap();
                assert_eq!(argument.kind(), "parenthesized");
                assert_eq!(argument.named_child(0).unwrap().kind(), "by");
            }
            _ => {}
        }
    }
    assert_eq!(checked, 6);
}

#[test]
fn low_precedence_call_arguments_require_parentheses() {
    for source in [
        "def bad : Nat := Nat.succ if true then 1 else 0",
        "def bad : Nat := Nat.succ match true with | true => 1 | false => 0",
        "theorem bad : 0 = 0 := id by rfl",
    ] {
        let tree = Parsers::new().parse(Language::Lean, source).unwrap();
        assert!(tree.has_errors(), "invalid Lean parsed cleanly: {source}");
    }
}

#[test]
fn negative_returns_and_binding_continuations_keep_their_signs() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    let mut cursor = tree.root().walk();
    let mut checked = 0;
    for definition in tree.root().named_children(&mut cursor) {
        if definition.kind() != "definition" {
            continue;
        }
        let name = definition.child_by_field_name("name").unwrap();
        let body = definition.child_by_field_name("body").unwrap();
        match &source[name.byte_range()] {
            "negativeDeclaration" => {
                checked += 1;
                assert_eq!(body.kind(), "unary_expression");
                assert_eq!(&source[body.byte_range()], "-n");
            }
            "negativeBindingBody" => {
                checked += 1;
                assert_eq!(body.kind(), "let");
                let returned = body.child_by_field_name("body").unwrap();
                assert_eq!(returned.kind(), "unary_expression");
                assert_eq!(&source[returned.byte_range()], "-value");
            }
            "subtractionContinuation" => {
                checked += 1;
                assert_eq!(body.kind(), "let");
                let value = body.child_by_field_name("value").unwrap();
                assert_eq!(value.kind(), "binary_expression");
                let operator = value.child_by_field_name("operator").unwrap();
                assert_eq!(&source[operator.byte_range()], "-");
                let returned = body.child_by_field_name("body").unwrap();
                assert_eq!(&source[returned.byte_range()], "value");
            }
            _ => {}
        }
    }
    assert_eq!(checked, 3);
}

#[test]
fn tactic_locations_preserve_later_source_declarations() {
    let source = include_str!("fixtures/lean-source-declarations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let (_directory, index) = common::workspace(&[("Source.lean", source)]);
    let expected: BTreeSet<_> = source
        .lines()
        .filter_map(|line| {
            line.strip_prefix("def ")
                .or_else(|| line.strip_prefix("theorem "))
        })
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    let actual: BTreeSet<_> = index
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Function)
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(actual, expected);
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
