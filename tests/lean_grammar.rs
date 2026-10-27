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
    assert_eq!(tree.root().to_sexp().matches("(let_bind ").count(), 2);
    assert_eq!(tree.root().to_sexp().matches("(tactic_rcases ").count(), 2);
    assert_eq!(
        tree.root()
            .to_sexp()
            .matches("(rcases_tuple_pattern ")
            .count(),
        1
    );
}

#[test]
fn tactic_comparisons_stay_inside_the_proof() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    let mut cursor = tree.root().walk();
    let mut checked = 0;
    for definition in tree.root().named_children(&mut cursor) {
        if definition.kind() != "definition" {
            continue;
        }
        let name = definition.child_by_field_name("name").unwrap();
        if !matches!(
            &source[name.byte_range()],
            "changeComparison" | "changeBeforeSplit"
        ) {
            continue;
        }
        checked += 1;
        let body = definition.child_by_field_name("body").unwrap();
        assert_eq!(body.kind(), "by", "{}", body.to_sexp());
        let tactic = body.named_child(0).unwrap();
        assert_eq!(tactic.kind(), "tactic_apply");
        assert_eq!(
            &source[tactic.child_by_field_name("tactic").unwrap().byte_range()],
            "change"
        );
        assert_eq!(
            tactic.child_by_field_name("arg").unwrap().kind(),
            "binary_expression"
        );
    }
    assert_eq!(checked, 2);
}

#[test]
fn show_terms_keep_their_type_and_proof_separate_from_show_tactics() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let mut cursor = tree.root().walk();
    let mut checked = 0;
    for definition in tree.root().named_children(&mut cursor) {
        if definition.kind() != "definition" {
            continue;
        }
        let name = definition.child_by_field_name("name").unwrap();
        let body = definition.child_by_field_name("body").unwrap();
        let term = match &source[name.byte_range()] {
            "showComparison" => {
                checked += 1;
                assert_eq!(body.kind(), "by");
                let tactic = body.named_child(0).unwrap();
                assert_eq!(tactic.kind(), "tactic_apply");
                let argument = tactic.child_by_field_name("arg").unwrap();
                assert_eq!(argument.kind(), "parenthesized");
                argument.named_child(0).unwrap()
            }
            "showFrom" => {
                checked += 1;
                body
            }
            "showThenTactic" => {
                checked += 1;
                assert_eq!(body.kind(), "by");
                assert_eq!(body.named_child(0).unwrap().kind(), "tactic_show");
                assert_eq!(body.named_child(1).unwrap().kind(), "tactic_apply");
                continue;
            }
            "afterShow" => {
                checked += 1;
                assert_eq!(body.kind(), "by");
                continue;
            }
            _ => continue,
        };
        assert_eq!(term.kind(), "show");
        assert_eq!(
            term.child_by_field_name("type").unwrap().kind(),
            "binary_expression"
        );
        let proof = term.child_by_field_name("body").unwrap();
        assert!(matches!(proof.kind(), "by" | "identifier"));
    }
    assert_eq!(checked, 4);
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
fn calculation_steps_and_following_tactics_keep_their_boundaries() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let mut cursor = tree.root().walk();
    let mut checked = BTreeSet::new();
    for definition in tree.root().named_children(&mut cursor) {
        let Some(name) = definition.child_by_field_name("name") else {
            continue;
        };
        let name = &source[name.byte_range()];
        if !matches!(
            name,
            "calcInline" | "calcNested" | "calcNegative" | "afterCalc"
        ) {
            continue;
        }
        checked.insert(name);
        let body = definition.child_by_field_name("body").unwrap();
        assert_eq!(body.kind(), "by");
        if name == "afterCalc" {
            continue;
        }
        let calc = if name == "calcNested" {
            let have = body.named_child(0).unwrap();
            assert_eq!(have.kind(), "tactic_have");
            assert_eq!(body.named_child(1).unwrap().kind(), "tactic_apply");
            have.child_by_field_name("value")
                .unwrap()
                .named_child(0)
                .unwrap()
        } else {
            let mut tactics = body.walk();
            body.named_children(&mut tactics).last().unwrap()
        };
        assert_eq!(calc.kind(), "tactic_calc");
        let mut steps = calc.walk();
        let steps = calc
            .named_children(&mut steps)
            .filter(|node| node.kind() == "calc_step")
            .collect::<Vec<_>>();
        assert_eq!(steps.len(), 2, "{}", calc.to_sexp());
        for step in steps {
            let mut children = step.walk();
            assert_eq!(
                step.named_children(&mut children)
                    .filter(|node| node.kind() != "comment")
                    .count(),
                2,
                "{}",
                step.to_sexp()
            );
        }
    }
    assert_eq!(
        checked,
        BTreeSet::from(["calcInline", "calcNested", "calcNegative", "afterCalc"])
    );
}

#[test]
fn invalid_call_arguments_and_goal_markers_report_syntax_errors() {
    for source in [
        "def bad : Nat := Nat.succ if true then 1 else 0",
        "def bad : Nat := Nat.succ match true with | true => 1 | false => 0",
        "theorem bad : 0 = 0 := id by rfl",
        "def bad (p q : Prop) : Prop := p ⊢ q",
        "theorem bad (p : Prop) : p := show p",
        "theorem bad (a : Int) : a = a := by\n  calc\n    a = a :=\n",
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
