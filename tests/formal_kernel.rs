use fun_refactor::formal_kernel::{evaluate, Failure, Operator, Term, Value};
use std::collections::BTreeMap;

fn int(value: i64) -> Term {
    Term::Value {
        value: Value::Int(value),
    }
}
fn boolean(value: bool) -> Term {
    Term::Value {
        value: Value::Bool(value),
    }
}
fn binary(operator: Operator, left: Term, right: Term) -> Term {
    Term::Binary {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
}

#[test]
fn binders_shadow_without_capturing_the_previous_environment() {
    let term = Term::Let {
        value: Box::new(int(3)),
        body: Box::new(binary(
            Operator::Add,
            Term::Bound { index: 0 },
            Term::Bound { index: 1 },
        )),
    };
    assert_eq!(evaluate(&term, &[Value::Int(7)], 64), Ok(Value::Int(10)));
    assert_eq!(
        evaluate(&Term::Bound { index: 1 }, &[], 64),
        Err(Failure::Unbound)
    );
}

#[test]
fn checked_arithmetic_and_partiality_have_explicit_boundaries() {
    for operator in [Operator::Add, Operator::Sub, Operator::Mul] {
        let right = if matches!(operator, Operator::Sub) {
            -1
        } else {
            2
        };
        assert_eq!(
            evaluate(&binary(operator, int(i64::MAX), int(right)), &[], 64),
            Err(Failure::Overflow)
        );
    }
    for operator in [Operator::Div, Operator::Rem] {
        assert_eq!(
            evaluate(&binary(operator, int(3), int(0)), &[], 64),
            Err(Failure::DivisionByZero)
        );
        assert_eq!(
            evaluate(&binary(operator, int(i64::MIN), int(-1)), &[], 64),
            Err(Failure::Overflow)
        );
    }
    assert_eq!(
        evaluate(&binary(Operator::Div, int(-7), int(3)), &[], 64),
        Ok(Value::Int(-2))
    );
    assert_eq!(
        evaluate(&binary(Operator::Rem, int(-7), int(3)), &[], 64),
        Ok(Value::Int(-1))
    );
    assert_eq!(
        evaluate(
            &Term::Neg {
                operand: Box::new(int(i64::MIN))
            },
            &[],
            64
        ),
        Err(Failure::Overflow)
    );
    assert_eq!(
        evaluate(&binary(Operator::Add, boolean(false), int(1)), &[], 64),
        Err(Failure::Type)
    );
}

#[test]
fn unused_boolean_and_conditional_branches_are_not_evaluated() {
    for (operator, value) in [(Operator::And, false), (Operator::Or, true)] {
        assert_eq!(
            evaluate(
                &binary(operator, boolean(value), Term::Bound { index: 99 }),
                &[],
                64
            ),
            Ok(Value::Bool(value))
        );
    }
    let term = Term::If {
        condition: Box::new(boolean(true)),
        then: Box::new(int(4)),
        otherwise: Box::new(Term::Bound { index: 99 }),
    };
    assert_eq!(evaluate(&term, &[], 64), Ok(Value::Int(4)));
    assert_eq!(evaluate(&term, &[], 1), Err(Failure::Fuel));
    assert_eq!(evaluate(&term, &[], 0), Err(Failure::Limit));
}

#[test]
fn products_sequences_options_and_results_preserve_their_structure() {
    let term = Term::Record {
        fields: BTreeMap::from([
            (
                "pair".into(),
                Term::Tuple {
                    items: vec![int(2), boolean(true)],
                },
            ),
            (
                "items".into(),
                Term::List {
                    items: vec![int(1), int(2)],
                },
            ),
            (
                "maybe".into(),
                Term::Option {
                    value: Some(Box::new(int(3))),
                },
            ),
            (
                "result".into(),
                Term::Result {
                    ok: false,
                    value: Box::new(int(4)),
                },
            ),
        ]),
    };
    let output = evaluate(&term, &[], 64).unwrap();
    let Value::Record(fields) = output else {
        panic!("record expected")
    };
    assert_eq!(
        fields["maybe"],
        Value::Option(Some(Box::new(Value::Int(3))))
    );
    assert_eq!(
        fields["result"],
        Value::Result {
            ok: false,
            value: Box::new(Value::Int(4))
        }
    );
    let get = Term::Field {
        value: Box::new(term.clone()),
        name: "items".into(),
    };
    assert_eq!(
        evaluate(
            &Term::Index {
                value: Box::new(get),
                index: 1
            },
            &[],
            64
        ),
        Ok(Value::Int(2))
    );
    assert_eq!(
        evaluate(
            &Term::Field {
                value: Box::new(term),
                name: "missing".into()
            },
            &[],
            64
        ),
        Err(Failure::MissingField)
    );
}

#[test]
fn generated_formal_plans_use_the_shared_ir_for_each_typed_reader() {
    let directory = tempfile::tempdir().unwrap();
    let fixtures = [
        ("checked.rs", "checked", "pub fn checked(value: bool) -> bool { value }\n"),
        ("checked.go", "checked", "package checks\nfunc checked(value bool) bool { return value }\n"),
        ("Checked.java", "Checked::checked", "public class Checked { public static boolean checked(boolean value) { return value; } }\n"),
        ("checked.py", "checked", "def checked(value: bool) -> bool:\n    return value\n"),
        ("checked.ts", "checked", "export function checked(value: boolean): boolean { return value; }\n"),
        ("checked.tsx", "checked", "export function checked(value: boolean): boolean { return value; }\n"),
        ("checked.zig", "checked", "pub fn checked(value: bool) bool { return value; }\n"),
        ("checked.lean", "checked", "def checked (value : Bool) : Bool := value\n"),
    ];
    for (path, symbol, source) in fixtures {
        std::fs::write(directory.path().join(path), source).unwrap();
        let target = format!("{path}::{symbol}");
        let plan = fun_refactor::spec::formal_plan(directory.path(), &target, &["identity".into()])
            .unwrap_or_else(|error| panic!("{path}: {error:#}"));
        assert_eq!(plan.kernel.output.lean_type, "Bool", "{path}");
        assert!(plan.kernel.lean_definition.contains("value"), "{path}");
        let task = fun_refactor::spec::property_task(directory.path(), &target, 4096).unwrap();
        assert_eq!(task.kernel.output.lean_type, "Bool", "{path}");
    }
    let candidates =
        fun_refactor::spec::formal_candidates(directory.path(), &[], 128, true).unwrap();
    assert_eq!(
        candidates
            .candidates
            .iter()
            .filter(|candidate| candidate.eligible)
            .count(),
        fixtures.len()
    );
}

#[test]
fn external_names_and_ill_typed_pure_bodies_refuse_before_model_generation() {
    let directory = tempfile::tempdir().unwrap();
    for (source, reason) in [
        (
            "pub fn checked(value: bool) -> bool { external }\n",
            "unbound",
        ),
        (
            "pub fn checked(value: i64) -> bool { !value }\n",
            "requires Bool",
        ),
        (
            "pub fn checked(value: bool) -> bool { value + value }\n",
            "operands",
        ),
    ] {
        std::fs::write(directory.path().join("checked.rs"), source).unwrap();
        let error = fun_refactor::spec::formal_plan(directory.path(), "checked.rs::checked", &[])
            .unwrap_err();
        assert!(error.to_string().contains(reason), "{error:#}");
        assert!(!directory.path().join(".fr-history").exists());
    }
}

fn run(root: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("-C")
        .arg(root)
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn each_typed_language_completes_agent_proofs_regeneration_drift_and_reversal() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), &["spec", "init", "--write"]);
    let fixtures = [
        ("keep.rs", "keep", "pub fn keep(value: bool) -> bool { value }\n"),
        ("keep.go", "keep", "package checks\nfunc keep(value bool) bool { return value }\n"),
        ("Keep.java", "Keep::keep", "class Other { static boolean keep(boolean value) { return !value; } }\n\npublic class Keep { public static boolean keep(boolean value) { return value; } }\n"),
        ("keep.py", "keep", "def keep(value: bool) -> bool:\n    return value\n"),
        ("keep.ts", "keep", "export function keep(value: boolean): boolean { return value; }\n"),
        ("keep.tsx", "keep", "export function keep(value: boolean): boolean { return value; }\n"),
        ("keep.zig", "keep", "pub fn keep(value: bool) bool { return value; }\n"),
        ("keep.lean", "keep", "def keep (value : Bool) : Bool := value\n"),
    ];
    for (path, symbol, source) in fixtures {
        std::fs::write(workspace.path().join(path), source).unwrap();
        let target = format!("{path}::{symbol}");
        let plan = fun_refactor::spec::formal_plan(
            workspace.path(),
            &target,
            &["identity".into(), "ir-model".into()],
        )
        .unwrap();
        std::fs::write(
            workspace.path().join("plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        run(
            workspace.path(),
            &["spec", "scaffold", "--from", "plan.json", "--write"],
        );
        let model = format!("specs/FrSpecs/{}.lean", plan.kernel.module);
        for property in &plan.properties {
            let goal = format!("{model}::{}", property.name);
            let task = run(workspace.path(), &["spec", "proof-task", &goal]);
            assert_eq!(task["schema"], "fr-proof-task-1");
            std::fs::write(
                workspace.path().join("proof.lean"),
                if property.kind == "ir-model" {
                    "cases value <;> rfl\n"
                } else {
                    "rfl\n"
                },
            )
            .unwrap();
            let attempt = run(
                workspace.path(),
                &["spec", "proof-check", &goal, "--from", "proof.lean"],
            );
            assert_eq!(attempt["passed"], true, "{path}: {attempt}");
            let before = std::fs::read(workspace.path().join(&model)).unwrap();
            let applied = run(
                workspace.path(),
                &["spec", "prove", &goal, "--from", "proof.lean", "--write"],
            );
            assert_eq!(attempt["receipt"], applied["receipt"]);
            let transaction = applied["transaction"].as_u64().unwrap().to_string();
            let after = std::fs::read(workspace.path().join(&model)).unwrap();
            assert_ne!(before, after);
            run(
                workspace.path(),
                &["history", "undo", &transaction, "--write"],
            );
            assert_eq!(
                std::fs::read(workspace.path().join(&model)).unwrap(),
                before
            );
            run(
                workspace.path(),
                &["history", "redo", &transaction, "--write"],
            );
            assert_eq!(std::fs::read(workspace.path().join(&model)).unwrap(), after);
        }
        let proved = std::fs::read(workspace.path().join(&model)).unwrap();
        let regenerated = fun_refactor::spec::scaffold_formal(
            workspace.path(),
            &workspace.path().join("plan.json"),
            std::path::Path::new("specs"),
        )
        .unwrap();
        assert!(regenerated.proof_bytes_preserved > 0);
        assert!(
            regenerated
                .files
                .iter()
                .all(|file| file.original == file.updated),
            "{path}: {:?}",
            regenerated.files
        );
        std::fs::write(workspace.path().join(path), format!("{source}\n")).unwrap();
        let fresh = fun_refactor::spec::formal_plan(
            workspace.path(),
            &target,
            &["identity".into(), "ir-model".into()],
        )
        .unwrap();
        assert_eq!(
            fresh.object_digest, plan.object_digest,
            "outside-declaration whitespace: {path}"
        );
        std::fs::write(
            workspace.path().join(path),
            source
                .replace("value }", "!value }")
                .replace("return value", "return !value")
                .replace(":= value", ":= !value"),
        )
        .unwrap();
        assert!(
            fun_refactor::spec::scaffold_formal(
                workspace.path(),
                &workspace.path().join("plan.json"),
                std::path::Path::new("specs")
            )
            .is_err(),
            "{path}"
        );
        assert_eq!(
            std::fs::read(workspace.path().join(&model)).unwrap(),
            proved
        );
        std::fs::write(workspace.path().join(path), source).unwrap();
    }
    run(workspace.path(), &["spec", "verify", "specs"]);
    let evidence = run(workspace.path(), &["spec", "evidence", "specs"]);
    let correspondence = evidence["kernel_correspondence"].as_array().unwrap();
    assert_eq!(correspondence.len(), fixtures.len());
    assert!(
        correspondence
            .iter()
            .all(|row| row["status"] == "checked_by_lean"
                && row["source_implementation_proved"] == false),
        "{correspondence:?}"
    );
    let library = workspace.path().join("specs/FrSpecs/PureKernel.lean");
    std::fs::write(
        &library,
        format!("{}\n", fun_refactor::formal_kernel::LEAN_SOURCE),
    )
    .unwrap();
    assert!(fun_refactor::spec::check_strict(workspace.path(), &["specs".into()], true).is_err());
}

#[test]
fn lean_execution_agrees_with_native_values_and_partiality() {
    use fun_refactor::formal_kernel::{quote_term, quote_value};
    let mut cases = Vec::new();
    let values = [i64::MIN, -7, -3, -1, 0, 1, 3, 7, i64::MAX];
    for operator in [
        Operator::Add,
        Operator::Sub,
        Operator::Mul,
        Operator::Div,
        Operator::Rem,
        Operator::Eq,
        Operator::Ne,
        Operator::Lt,
        Operator::Le,
        Operator::Gt,
        Operator::Ge,
    ] {
        for left in values {
            for right in values {
                cases.push((binary(operator, int(left), int(right)), vec![], 64));
            }
        }
    }
    for operator in [Operator::And, Operator::Or, Operator::Xor] {
        for left in [false, true] {
            for right in [false, true] {
                cases.push((binary(operator, boolean(left), boolean(right)), vec![], 64));
            }
        }
    }
    cases.extend([
        (
            Term::Value {
                value: Value::String("\u{0008}\u{000c}\u{0000}λ🙂\\b\"".into()),
            },
            vec![],
            64,
        ),
        (
            Term::Field {
                value: Box::new(Term::Record {
                    fields: BTreeMap::from([("\u{0008}".into(), int(3))]),
                }),
                name: "\u{0008}".into(),
            },
            vec![],
            64,
        ),
        (
            Term::Let {
                value: Box::new(int(3)),
                body: Box::new(Term::Bound { index: 1 }),
            },
            vec![Value::Int(7)],
            64,
        ),
        (Term::Bound { index: 10 }, vec![], 64),
        (int(1), vec![], 1),
        (
            Term::Not {
                operand: Box::new(boolean(true)),
            },
            vec![],
            1,
        ),
        (
            Term::Neg {
                operand: Box::new(int(i64::MIN)),
            },
            vec![],
            64,
        ),
        (
            Term::Record {
                fields: BTreeMap::from([
                    (
                        "a".into(),
                        Term::List {
                            items: vec![int(3)],
                        },
                    ),
                    ("b".into(), Term::Option { value: None }),
                    (
                        "c".into(),
                        Term::Result {
                            ok: false,
                            value: Box::new(int(4)),
                        },
                    ),
                    (
                        "d".into(),
                        Term::Tuple {
                            items: vec![int(1), boolean(true)],
                        },
                    ),
                ]),
            },
            vec![],
            64,
        ),
        (
            Term::Field {
                value: Box::new(Term::Record {
                    fields: BTreeMap::new(),
                }),
                name: "missing".into(),
            },
            vec![],
            64,
        ),
        (
            Term::Index {
                value: Box::new(Term::List {
                    items: vec![int(1)],
                }),
                index: 3,
            },
            vec![],
            64,
        ),
    ]);
    let workspace = tempfile::tempdir().unwrap();
    let mut source = fun_refactor::formal_kernel::LEAN_SOURCE.to_owned();
    for (index, (term, environment, fuel)) in cases.iter().enumerate() {
        let expected = evaluate(term, environment, *fuel)
            .ok()
            .map_or("none".into(), |value| {
                format!("some {}", quote_value(&value))
            });
        let environment = environment
            .iter()
            .map(quote_value)
            .collect::<Vec<_>>()
            .join(", ");
        source.push_str(&format!(
            "\nexample : (FrPureKernel.eval {fuel} [{environment}] {} == {expected}) = true := by native_decide\n",
            quote_term(term)
        ));
        assert!(index < 1024);
    }
    let path = workspace.path().join("Corpus.lean");
    std::fs::write(&path, source).unwrap();
    let output = std::process::Command::new("lake")
        .args(["env", "lean"])
        .arg(path)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn typed_source_execution_matches_the_boolean_kernel_truth_table() {
    let workspace = tempfile::tempdir().unwrap();
    let fixtures = [
        ("main.rs", "both", "pub fn both(left: bool, right: bool) -> bool { left && right }\n", "fn main() { for left in [false, true] { for right in [false, true] { println!(\"{}\", both(left, right)); } } }\n", "rust"),
        ("main.go", "both", "package main\nimport \"fmt\"\nfunc both(left bool, right bool) bool { return left && right }\n", "func main() { for _, left := range []bool{false, true} { for _, right := range []bool{false, true} { fmt.Println(both(left, right)) } } }\n", "go"),
        ("Main.java", "Main::both", "public class Main { public static boolean both(boolean left, boolean right) { return left && right; }\n", "public static void main(String[] args) { for (boolean left : new boolean[]{false,true}) { for (boolean right : new boolean[]{false,true}) { System.out.println(both(left,right)); } } } }\n", "java"),
        ("main.py", "both", "def both(left: bool, right: bool) -> bool:\n    return left and right\n", "for left in (False, True):\n    for right in (False, True):\n        print(str(both(left, right)).lower())\n", "python"),
        ("main.ts", "both", "export function both(left: boolean, right: boolean): boolean { return left && right; }\n", "for (const left of [false,true]) { for (const right of [false,true]) { console.log(both(left,right)); } }\n", "typescript"),
        ("main.zig", "both", "const std = @import(\"std\");\npub fn both(left: bool, right: bool) bool { return left and right; }\n", "pub fn main() void { for ([_]bool{false,true}) |left| { for ([_]bool{false,true}) |right| { std.debug.print(\"{}\\n\", .{both(left,right)}); } } }\n", "zig"),
        ("main.lean", "both", "def both (left : Bool) (right : Bool) : Bool := left && right\n", "def main : IO Unit := do\n  for left in [false, true] do\n    for right in [false, true] do\n      IO.println (both left right)\n", "lean"),
    ];
    for (path, symbol, declaration, driver, language) in fixtures {
        let directory = workspace.path().join(language);
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join(path);
        std::fs::write(&source, format!("{declaration}{driver}")).unwrap();
        let plan = fun_refactor::spec::formal_plan(
            &directory,
            &format!("{path}::{symbol}"),
            &["ir-model".into()],
        )
        .unwrap_or_else(|error| panic!("{language}: {error:#}"));
        let term = &plan.kernel.evaluation.as_ref().unwrap().term;
        let mut expected = Vec::new();
        for left in [false, true] {
            for right in [false, true] {
                let value = evaluate(term, &[Value::Bool(left), Value::Bool(right)], 256).unwrap();
                assert_eq!(value, Value::Bool(left && right));
                expected.push((left && right).to_string());
            }
        }
        let execute = |command: &mut std::process::Command| {
            let output = command.current_dir(&directory).output().unwrap();
            assert!(
                output.status.success(),
                "{language}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };
        let output = match language {
            "rust" => {
                execute(
                    std::process::Command::new("rustc")
                        .arg(&source)
                        .arg("-o")
                        .arg(directory.join("program")),
                );
                execute(&mut std::process::Command::new(directory.join("program")))
            }
            "go" => execute(
                std::process::Command::new("go")
                    .arg("run")
                    .arg(&source)
                    .env("GO111MODULE", "off")
                    .env(
                        "GOCACHE",
                        concat!(env!("CARGO_MANIFEST_DIR"), "/target/go-cache"),
                    ),
            ),
            "java" => execute(std::process::Command::new("java").arg(&source)),
            "python" => execute(std::process::Command::new("python3").arg(&source)),
            "typescript" => {
                execute(
                    std::process::Command::new("tsc")
                        .args(["--target", "es2020", "--module", "commonjs", "--strict"])
                        .arg(&source),
                );
                execute(std::process::Command::new("node").arg(directory.join("main.js")))
            }
            "zig" => execute(
                std::process::Command::new("zig")
                    .arg("run")
                    .arg(&source)
                    .env(
                        "ZIG_GLOBAL_CACHE_DIR",
                        concat!(env!("CARGO_MANIFEST_DIR"), "/target/zig-cache"),
                    ),
            ),
            "lean" => {
                let output = std::process::Command::new("lake")
                    .args(["env", "lean", "--run"])
                    .arg(&source)
                    .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/kernels"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                output
            }
            _ => unreachable!(),
        };
        let values = if language == "zig" {
            &output.stderr
        } else {
            &output.stdout
        };
        assert_eq!(
            String::from_utf8_lossy(values).lines().collect::<Vec<_>>(),
            expected,
            "{language}"
        );
    }
}

#[test]
fn declarative_languages_complete_structural_proof_and_history_workflows() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), &["spec", "init", "--write"]);
    let fixtures = [
        (
            "page.html",
            "<main id=\"content\"><p class=\"text-blue-500\">Hello</p></main>\n",
        ),
        ("style.css", ".card { color: blue; }\n"),
        ("style.scss", "$color: blue;\n.card { color: $color; }\n"),
        ("style.sass", ".card\n  color: blue\n"),
        (
            "infra.hcl",
            "resource \"local_file\" \"note\" { content = \"hello\" }\n",
        ),
        ("data.json", "{\"app\": {\"enabled\": true}}\n"),
        ("config.yaml", "app:\n  enabled: true\n"),
        (
            "templates/config.yaml",
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {{ .Values.name }}\n",
        ),
        ("document.xml", "<project><name>Example</name></project>\n"),
        ("notes.md", "# Notes\n\n```mermaid\ngraph TD\n A-->B\n```\n"),
    ];
    for (path, source) in fixtures {
        let source_path = workspace.path().join(path);
        std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        std::fs::write(&source_path, source).unwrap();
        let target = format!("{path}::__fr_structure__");
        let candidates =
            fun_refactor::spec::formal_candidates(workspace.path(), &[path.into()], 128, true)
                .unwrap();
        let candidate = candidates
            .candidates
            .iter()
            .find(|candidate| candidate.target == target)
            .unwrap();
        assert!(candidate.eligible, "{path}: {}", candidate.reason);
        assert!(candidate
            .suggested_properties
            .contains(&"retained-facts-wellformed"));
        let plan = fun_refactor::spec::formal_plan(
            workspace.path(),
            &target,
            &["retained-facts-wellformed".into(), "ir-model".into()],
        )
        .unwrap();
        assert_eq!(
            plan.correspondence.signature_surface,
            "retained-structure-bool-map"
        );
        assert_eq!(plan.kernel.semantic_ir["runtime_semantics"], false);
        std::fs::write(
            workspace.path().join("structural-plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        run(
            workspace.path(),
            &[
                "spec",
                "scaffold",
                "--from",
                "structural-plan.json",
                "--write",
            ],
        );
        for property in &plan.properties {
            let target = format!(
                "specs/FrSpecs/{}.lean::{}",
                plan.kernel.module, property.name
            );
            std::fs::write(workspace.path().join("proof.lean"), "rfl\n").unwrap();
            let attempt = run(
                workspace.path(),
                &["spec", "proof-check", &target, "--from", "proof.lean"],
            );
            assert_eq!(attempt["passed"], true, "{path}: {attempt}");
            let applied = run(
                workspace.path(),
                &["spec", "prove", &target, "--from", "proof.lean", "--write"],
            );
            let transaction = applied["transaction"].as_u64().unwrap().to_string();
            run(
                workspace.path(),
                &["history", "undo", &transaction, "--write"],
            );
            run(
                workspace.path(),
                &["history", "redo", &transaction, "--write"],
            );
        }
        let generated = fun_refactor::spec::scaffold_formal(
            workspace.path(),
            &workspace.path().join("structural-plan.json"),
            std::path::Path::new("specs"),
        )
        .unwrap();
        assert!(generated
            .files
            .iter()
            .all(|file| file.original == file.updated));
        std::fs::write(&source_path, format!("{source}\n")).unwrap();
        assert!(
            fun_refactor::spec::scaffold_formal(
                workspace.path(),
                &workspace.path().join("structural-plan.json"),
                std::path::Path::new("specs")
            )
            .is_err(),
            "whole-file drift: {path}"
        );
        std::fs::write(&source_path, source).unwrap();
    }
    run(workspace.path(), &["spec", "verify", "specs"]);
}

#[test]
fn untyped_script_and_framework_constructs_report_their_formal_boundary() {
    let workspace = tempfile::tempdir().unwrap();
    for (path, source, reason) in [
        (
            "script.js",
            "export function keep(value) { return value; }\n",
            "explicit supported type",
        ),
        (
            "component.jsx",
            "export function Card(props) { return <p>{props.value}</p>; }\n",
            "explicit supported type",
        ),
        (
            "script.sh",
            "keep() { printf '%s' \"$1\"; }\n",
            "explicit supported type",
        ),
        (
            "route.ts",
            "export async function GET(value: boolean): Promise<boolean> { return value; }\n",
            "async",
        ),
        (
            "handlers.py",
            "async def keep(value: bool) -> bool:\n    return value\n",
            "async",
        ),
    ] {
        std::fs::write(workspace.path().join(path), source).unwrap();
        let report =
            fun_refactor::spec::formal_candidates(workspace.path(), &[path.into()], 128, true)
                .unwrap();
        assert!(
            !report.candidates.is_empty(),
            "{path}: exclusions must not disappear"
        );
        assert!(report
            .candidates
            .iter()
            .all(|candidate| !candidate.eligible));
        assert!(
            report
                .candidates
                .iter()
                .any(|candidate| candidate.reason.contains(reason)),
            "{path}: {:?}",
            report.candidates
        );
    }
}
