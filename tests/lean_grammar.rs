use fun_refactor::lang::Language;
use fun_refactor::model::SymbolKind;
use fun_refactor::parse::Parsers;
use std::collections::BTreeSet;

mod common;

#[test]
fn introductions_keep_discarded_hypotheses_and_simplifier_phases() {
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
        match name {
            "introduceAndDiscard" | "introduceTyped" | "introduceAlternatives" => {
                let body = definition.child_by_field_name("body").unwrap();
                let introduction = body.named_child(0).unwrap();
                assert_eq!(introduction.kind(), "tactic_rintro");
                let pattern = introduction.child_by_field_name("pattern").unwrap();
                let expected = match name {
                    "introduceAndDiscard" => "rcases_tuple_pattern",
                    "introduceTyped" => "rintro_binder",
                    _ => "rcases_alternative_pattern",
                };
                assert_eq!(pattern.kind(), expected);
                if name == "introduceTyped" {
                    assert_eq!(
                        &source[pattern.child_by_field_name("type").unwrap().byte_range()],
                        "Nat"
                    );
                }
                if name == "introduceAlternatives" {
                    assert!(pattern.to_sexp().contains("rcases_alternative_pattern"));
                }
                if name == "introduceAndDiscard" {
                    assert_eq!(&source[pattern.byte_range()], "⟨ha, -, -, -⟩");
                    assert_eq!(body.named_child(1).unwrap().kind(), "tactic_apply");
                }
            }
            "simplifyBeforeArguments" => {
                let body = definition.child_by_field_name("body").unwrap();
                assert!(body.to_sexp().contains("(simp_phase"));
            }
            "afterIntroductionSyntax" => {}
            _ => continue,
        }
        checked.insert(name);
    }
    assert_eq!(checked.len(), 5);
    let (_directory, index) = common::workspace(&[("Introductions.lean", source)]);
    for name in checked {
        assert_eq!(
            index.symbols.iter().filter(|s| s.name == name).count(),
            1,
            "{name}"
        );
    }
}

#[test]
fn dated_attributes_preserve_their_arguments_and_following_declarations() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let mut cursor = tree.root().walk();
    let declaration = tree
        .root()
        .named_children(&mut cursor)
        .find(|node| {
            node.kind() == "declaration" && source[node.byte_range()].contains("libraryAlias")
        })
        .unwrap();
    let attributes = declaration.child_by_field_name("attributes").unwrap();
    let entry = attributes.named_child(0).unwrap();
    let mut cursor = entry.walk();
    let argument = entry
        .named_children(&mut cursor)
        .find(|node| node.kind() == "named_argument")
        .unwrap();
    assert_eq!(argument.kind(), "named_argument");
    assert_eq!(
        &source[argument.child_by_field_name("name").unwrap().byte_range()],
        "since"
    );
    assert_eq!(
        &source[argument.child_by_field_name("value").unwrap().byte_range()],
        "\"2025-09-01\""
    );
    let (_directory, index) = common::workspace(&[("Library.lean", source)]);
    for name in ["libraryReplacement", "libraryAlias", "afterAttributeSyntax"] {
        assert_eq!(
            index
                .symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Function && s.name == name)
                .count(),
            1,
            "{name}"
        );
    }
}

#[test]
fn installed_integer_library_exposes_division_facts() {
    let prefix = std::process::Command::new("lean")
        .arg("--print-prefix")
        .output()
        .expect("the Lean grammar gate requires the pinned Lean installation");
    assert!(prefix.status.success());
    let source_path = std::path::PathBuf::from(String::from_utf8(prefix.stdout).unwrap().trim())
        .join("src/lean/Init/Data/Int/DivMod/Lemmas.lean");
    let source = std::fs::read_to_string(source_path).unwrap();
    let (_directory, index) = common::workspace(&[("Lemmas.lean", &source)]);
    for name in [
        "tdiv_eq_ediv_of_nonneg",
        "mul_tdiv_add_tmod",
        "tdiv_mul_add_tmod",
        "tdiv_nonneg",
        "tmod_nonneg",
        "tmod_lt_of_pos",
    ] {
        let found: Vec<_> = index
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function && s.name == name)
            .collect();
        assert_eq!(found.len(), 1, "missing or repeated division fact: {name}");
        let declaration = &source[found[0].full_span.start..found[0].full_span.end];
        assert!(declaration.starts_with("theorem "), "{declaration}");
        assert!(declaration.contains(name));
        assert!(
            !declaration.contains("\ntheorem "),
            "{name} consumed another theorem"
        );
    }
}

#[test]
fn library_lambda_and_obtain_branches_preserve_following_declarations() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let mut cursor = tree.root().walk();
    let definitions: Vec<_> = tree.root().named_children(&mut cursor).collect();
    let mut checked = BTreeSet::new();
    for definition in definitions {
        let Some(name) = definition.child_by_field_name("name") else {
            continue;
        };
        let name = &source[name.byte_range()];
        match name {
            "unicodeLambda" => {
                let body = definition.child_by_field_name("body").unwrap();
                assert_eq!(body.kind(), "fun");
                assert_eq!(
                    &source[body.child_by_field_name("body").unwrap().byte_range()],
                    "n"
                );
            }
            "obtainBranches" | "obtainTupleBranch" => {
                let body = definition.child_by_field_name("body").unwrap();
                let binding = body.named_child(0).unwrap();
                assert_eq!(binding.kind(), "tactic_have");
                assert_eq!(
                    binding.child_by_field_name("name").unwrap().kind(),
                    "rcases_alternative_pattern"
                );
                assert_eq!(body.named_child_count(), 3, "{}", body.to_sexp());
            }
            "afterBranchSyntax" => {}
            _ => continue,
        }
        checked.insert(name);
    }
    assert_eq!(checked.len(), 4);
    let (_directory, index) = common::workspace(&[("Library.lean", source)]);
    for name in checked {
        assert_eq!(
            index
                .symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Function && s.name == name)
                .count(),
            1,
            "{name}"
        );
    }
}

#[test]
fn subscript_names_and_conversions_keep_their_full_tokens_and_operands() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let mut cursor = tree.root().walk();
    let mut checked = BTreeSet::new();
    for definition in tree.root().named_children(&mut cursor) {
        if definition.kind() != "definition" {
            continue;
        }
        let name = definition.child_by_field_name("name").unwrap();
        let name = &source[name.byte_range()];
        let body = definition.child_by_field_name("body").unwrap();
        let (conversion, operator, operand) = match name {
            "indexedClaim₁" | "afterLibrarySyntax" => {
                checked.insert(name);
                continue;
            }
            "castArgument" => {
                assert_eq!(body.kind(), "application");
                (body.child_by_field_name("arguments").unwrap(), "↑", "n")
            }
            "castSum" => {
                assert_eq!(body.kind(), "binary_expression");
                (body.child_by_field_name("left").unwrap(), "↑", "n")
            }
            "functionCoercion" => {
                assert_eq!(body.kind(), "application");
                (body.child_by_field_name("name").unwrap(), "⇑", "f")
            }
            "typeCoercion" => (body, "↥", "c"),
            _ => continue,
        };
        checked.insert(name);
        assert_eq!(conversion.kind(), "coercion", "{name}");
        assert_eq!(
            &source[conversion
                .child_by_field_name("operator")
                .unwrap()
                .byte_range()],
            operator,
            "{name}"
        );
        assert_eq!(
            &source[conversion
                .child_by_field_name("operand")
                .unwrap()
                .byte_range()],
            operand,
            "{name}"
        );
    }
    assert_eq!(checked.len(), 6);
    assert!(tree.root().to_sexp().contains("(synthetic_hole)"));
}

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
        3
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
fn typed_proofs_keep_their_annotations_and_case_labels() {
    let source = include_str!("fixtures/lean-proof-locations.lean");
    let tree = Parsers::new().parse(Language::Lean, source).unwrap();
    assert!(!tree.has_errors(), "{}", tree.root().to_sexp());
    let mut cursor = tree.root().walk();
    let mut checked = 0;
    for definition in tree.root().named_children(&mut cursor) {
        let Some(name) = definition.child_by_field_name("name") else {
            continue;
        };
        let name = &source[name.byte_range()];
        if !matches!(
            name,
            "typedProof" | "typedCaseLabel" | "typedByCases" | "typedCalc"
        ) {
            continue;
        }
        checked += 1;
        let body = definition.child_by_field_name("body").unwrap();
        assert_eq!(body.kind(), "by");
        let exact = body.named_child(0).unwrap();
        let argument = exact.child_by_field_name("arg").unwrap();
        assert_eq!(argument.kind(), "parenthesized");
        assert!(argument.child_by_field_name("type").is_some());
        let proof = argument.named_child(0).unwrap();
        assert_eq!(proof.kind(), "by");
        let tactic = proof.named_child(0).unwrap();
        match name {
            "typedCaseLabel" => {
                assert_eq!(tactic.kind(), "tactic_cases");
                assert_eq!(
                    &source[tactic.child_by_field_name("equation").unwrap().byte_range()],
                    "h"
                );
            }
            "typedByCases" => {
                assert_eq!(tactic.kind(), "tactic_by_cases");
                assert_eq!(
                    &source[tactic.child_by_field_name("name").unwrap().byte_range()],
                    "hp"
                );
            }
            "typedCalc" => assert_eq!(tactic.kind(), "tactic_calc"),
            _ => assert_eq!(tactic.kind(), "tactic_apply"),
        }
    }
    assert_eq!(checked, 4);
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
        "theorem bad : True := by exact (by trivial : )",
        "def bad : Nat → Nat := fun n ↦",
        "theorem bad (h : True) : True := by obtain hp | := h",
        "@[deprecated replacement (since := )] def bad : Nat := 0",
        "theorem bad (h : True) : True := by have hp | hq := h",
        "theorem bad (h : True) : True := by suffices hp | hq := h",
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
