use fun_refactor::{
    edit::{apply_to_string, EditSet},
    index::Index,
    refactor::signature::{self, Change},
    scan::ScanOptions,
};
use std::{fs, path::Path, process::Command};

fn workspace(file: &str, source: &str) -> (tempfile::TempDir, Index) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(file), source).unwrap();
    let index = Index::build(dir.path(), &ScanOptions::default()).unwrap();
    (dir, index)
}

fn changed(index: &Index, change: Change) -> anyhow::Result<signature::SignaturePlan> {
    let id = index.find_symbols("adjust", None)[0].id;
    signature::change(index, id, change)
}

fn apply(root: &Path, file: &str, edits: &EditSet) -> String {
    let path = root.join(file);
    let before = fs::read_to_string(&path).unwrap();
    apply_to_string(&before, edits.edits_for(&path).unwrap()).unwrap()
}

fn output(program: &str, source: &str) -> Vec<u8> {
    let result = Command::new(program).args(["-c", source]).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

#[test]
fn nonadjacent_python_moves_preserve_binding_and_intervening_order() {
    let source = "def adjust(a, b, c, d):\n    return a * 1000 + b * 100 + c * 10 + d\nprint(adjust(1, 2, 3, 4))\n";
    for (from, to, declaration, call) in [
        (0, 2, "def adjust(b, c, a, d)", "adjust(2, 3, 1, 4)"),
        (3, 0, "def adjust(d, a, b, c)", "adjust(4, 1, 2, 3)"),
    ] {
        let (dir, index) = workspace("subject.py", source);
        let plan = changed(&index, Change::Move { from, to }).unwrap();
        let after = apply(dir.path(), "subject.py", &plan.edits);
        assert!(after.contains(declaration), "{after}");
        assert!(after.contains(call), "{after}");
        assert_eq!(output("python3", source), output("python3", &after));
    }
}

#[test]
fn python_removal_follows_keyword_binding_and_keeps_retained_effects() {
    for source in [
        "def adjust(a=1, unused=0, c=3):\n    return a + c\nprint(adjust(unused=2))\n",
        "def adjust(a, unused, c):\n    return a + c\nprint(adjust(1, 2, c=3))\n",
        "def adjust(a, unused=0, c=3):\n    return a + c\nprint(adjust(c=3, unused=2, a=1))\n",
        "def record():\n    print('called')\n    return 1\ndef adjust(a, unused=0, c=3):\n    return a + c\nprint(adjust(record(), 2, c=3))\n",
    ] {
        let (dir, index) = workspace("subject.py", source);
        let plan = changed(&index, Change::Remove(1)).unwrap();
        let after = apply(dir.path(), "subject.py", &plan.edits);
        assert!(!after.contains("unused"), "{after}");
        assert_eq!(output("python3", source), output("python3", &after));
    }
}

#[test]
fn effectful_removal_refuses_in_each_programming_language() {
    for (file, source) in [
        ("a.rs", "fn adjust(unused: i32) {}\nfn record() -> i32 { 1 }\nfn run() { adjust(record()); }\n"),
        ("a.py", "def adjust(unused):\n    pass\ndef record():\n    return 1\nadjust(record())\n"),
        ("a.ts", "function adjust(unused: number) {}\nfunction record() { return 1; }\nadjust(record());\n"),
        ("a.tsx", "function adjust(unused: number) {}\nfunction record() { return 1; }\nadjust(record());\nconst view = <div/>;\n"),
        ("a.go", "package main\nfunc adjust(unused int) {}\nfunc record() int { return 1 }\nfunc main() { adjust(record()) }\n"),
        ("A.java", "class A { static void adjust(int unused) {} static int record() { return 1; } static void run() { adjust(record()); } }\n"),
        ("a.zig", "fn adjust(unused: i32) void {}\nfn record() i32 { return 1; }\nfn run() void { adjust(record()); }\n"),
    ] {
        let (dir, index) = workspace(file, source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(error.contains("evaluation"), "{file}: {error}");
        assert_eq!(fs::read_to_string(dir.path().join(file)).unwrap(), source);
    }
}

#[test]
fn moving_across_an_effectful_middle_argument_refuses() {
    let source = "def adjust(a, b, c):\n    return a + b + c\ndef record():\n    return 2\nadjust(1, record(), 3)\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(&index, Change::Move { from: 0, to: 2 })
        .unwrap_err()
        .to_string();
    assert!(error.contains("evaluation"), "{error}");
}

#[test]
fn one_member_read_can_cross_literals_or_an_isolated_parameter() {
    for (file, program, source) in [
        (
            "a.py",
            "python3",
            include_str!("fixtures/signature/member-read.py"),
        ),
        (
            "a.js",
            "node",
            include_str!("fixtures/signature/member-read.js"),
        ),
    ] {
        let (dir, index) = workspace(file, source);
        let plan = changed(&index, Change::Move { from: 0, to: 1 }).unwrap();
        let after = apply(dir.path(), file, &plan.edits);
        let execute = |text: &str| {
            let flag = if program == "node" { "-e" } else { "-c" };
            let result = Command::new(program).args([flag, text]).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            result.stdout
        };
        assert_eq!(execute(source), execute(&after));
        assert_eq!(
            execute(&after)
                .split(|byte| *byte == b'\n')
                .filter(|line| *line == b"read")
                .count(),
            2
        );
    }
}

#[test]
fn member_reads_do_not_cross_mutable_bindings_or_other_member_reads() {
    for source in [
        include_str!("fixtures/signature/member-captured-write.js"),
        include_str!("fixtures/signature/member-global-read.js"),
        include_str!("fixtures/signature/two-member-reads.js"),
    ] {
        let (_dir, index) = workspace("a.js", source);
        let error = changed(&index, Change::Move { from: 0, to: 1 })
            .unwrap_err()
            .to_string();
        assert!(error.contains("evaluation"), "{error}");
    }
}

#[test]
fn a_record_callback_is_not_an_unrelated_method() {
    for (file, source) in [
        ("a.go", "package main\nfunc adjust(a int, b int) int { return a * 10 + b }\ntype Held struct { adjust func(int, int) int }\nfunc main() { h := Held{adjust: adjust}; _ = h.adjust(1, 2) }\n"),
        ("a.zig", "fn adjust(a: i32, b: i32) i32 { return a * 10 + b; }\nconst Held = struct { adjust: *const fn (i32, i32) i32 };\nfn run() void { const h = Held{ .adjust = &adjust }; _ = h.adjust(1, 2); }\n"),
    ] {
        let (_dir, index) = workspace(file, source);
        let id = index.find_symbols("adjust", None).into_iter()
            .find(|symbol| symbol.kind == fun_refactor::model::SymbolKind::Function && symbol.qualifier.is_none()).unwrap().id;
        assert!(signature::change(&index, id, Change::Move { from: 0, to: 1 }).is_err());
    }
}

#[test]
fn omitted_defaults_cannot_silently_change_positions() {
    let source = "def adjust(a=1, b=2, c=3):\n    return a * 100 + b * 10 + c\nprint(adjust(9))\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(&index, Change::Move { from: 0, to: 2 })
        .unwrap_err()
        .to_string();
    assert!(error.contains("default binding"), "{error}");
}

#[test]
fn removed_defaults_and_annotations_cannot_hide_effects() {
    for parameter in ["unused=record()", "unused: record() = 0"] {
        let source = format!("def record():\n    print('effect')\n    return int\ndef adjust({parameter}):\n    return 1\nprint(adjust())\n");
        let (_dir, index) = workspace("a.py", &source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(
            error.contains("evaluation") || error.contains("annotation"),
            "{error}"
        );
    }
}


#[test]
fn rust_macro_calls_use_the_same_evaluation_rule() {
    for argument in ["record()", "{ record(); 0 }"] {
        let source = format!("fn adjust(unused: i32) -> i32 {{ 1 }}\nfn record() -> i32 {{ 0 }}\nfn run() {{ assert_eq!(adjust({argument}), 1); }}\n");
        let (_dir, index) = workspace("a.rs", &source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(error.contains("evaluation"), "{error}");
    }
    let source = "fn adjust(unused: i32) -> i32 { 1 }\nfn run() { assert_eq!(adjust(-2), 1); }\n";
    let (dir, index) = workspace("a.rs", source);
    let plan = changed(&index, Change::Remove(0)).unwrap();
    assert!(apply(dir.path(), "a.rs", &plan.edits).contains("assert_eq!(adjust(), 1)"));
}

#[test]
fn shell_moves_shift_positions_and_preserve_output() {
    let source =
        "adjust() { printf '%s:%s:%s\\n' \"$1\" \"$2\" \"$3\"; }\nadjust alpha beta gamma\n";
    for (from, to) in [(0, 2), (2, 0)] {
        let (dir, index) = workspace("a.sh", source);
        let plan = changed(&index, Change::Move { from, to }).unwrap();
        let after = apply(dir.path(), "a.sh", &plan.edits);
        assert_eq!(output("bash", source), output("bash", &after));
    }
}

#[test]
fn quoted_shell_substitutions_keep_their_effects() {
    for argument in [
        "\"$(printf effect)\"",
        "\"$((counter += 1))\"",
        "\"${value:=changed}\"",
        "\"$RANDOM\"",
        "\"$SRANDOM\"",
    ] {
        let source = format!("adjust() {{ printf '%s' \"$1\"; }}\nadjust kept {argument}\n");
        let (_dir, index) = workspace("a.sh", &source);
        let error = changed(&index, Change::Remove(1)).unwrap_err().to_string();
        assert!(error.contains("evaluation"), "{error}");
    }
}

#[test]
fn added_parameters_cannot_capture_existing_names_or_ignore_positions() {
    let source = "LIMIT = 4\ndef adjust(a):\n    return a + LIMIT\nprint(adjust(1))\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(
        &index,
        Change::Add {
            at: 1,
            declaration: "LIMIT".into(),
            argument: "0".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("means something else"), "{error}");
    let error = changed(
        &index,
        Change::Add {
            at: 4,
            declaration: "unused".into(),
            argument: "0".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("position 4"), "{error}");
}

#[test]
fn adding_a_required_python_parameter_after_a_default_refuses() {
    let source = "def adjust(a=1):\n    return a\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(
        &index,
        Change::Add {
            at: 1,
            declaration: "b".into(),
            argument: "2".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("required parameter after a default"),
        "{error}"
    );
}

#[test]
fn mutable_rust_parameters_cannot_be_added_twice() {
    let source = "fn adjust(mut value: i32) -> i32 { value += 1; value }\n";
    let (_dir, index) = workspace("a.rs", source);
    let error = changed(
        &index,
        Change::Add {
            at: 1,
            declaration: "value: i32".into(),
            argument: "0".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("already has a parameter"), "{error}");
}

#[test]
fn an_unchanged_position_still_has_to_exist() {
    let (_dir, index) = workspace("a.py", "def adjust(value):\n    return value\n");
    let error = changed(&index, Change::Move { from: 7, to: 7 })
        .unwrap_err()
        .to_string();
    assert!(error.contains("not both present"), "{error}");
}

#[test]
fn shell_positions_refuse_overflow_instead_of_panicking() {
    let (_dir, index) = workspace("a.sh", "adjust() { echo \"$1\"; }\nadjust 1\n");
    for change in [
        Change::Remove(usize::MAX),
        Change::Move {
            from: 0,
            to: usize::MAX,
        },
        Change::Add {
            at: usize::MAX,
            declaration: String::new(),
            argument: "0".into(),
        },
    ] {
        let error = changed(&index, change).unwrap_err().to_string();
        assert!(error.contains("position"), "{error}");
    }
}

#[test]
fn a_sass_addition_cannot_fill_an_earlier_omitted_parameter() {
    let source = "@mixin adjust($fg: white) { color: $fg; }\n.a { @include adjust(); }\n";
    let (_dir, index) = workspace("a.scss", source);
    let error = changed(
        &index,
        Change::Add {
            at: 1,
            declaration: "$bg".into(),
            argument: "black".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("cannot insert at position 1"), "{error}");
}

#[test]
fn typescript_default_expressions_keep_their_evaluation() {
    for (parameters, change) in [
        ("unused = record()", Change::Remove(0)),
        (
            "left = record(), right = record()",
            Change::Move { from: 0, to: 1 },
        ),
    ] {
        let source = format!("function record() {{ console.log('effect'); return 1; }}\nfunction adjust({parameters}) {{ return 0; }}\nadjust();\n");
        let (_dir, index) = workspace("a.ts", &source);
        let error = changed(&index, change).unwrap_err().to_string();
        assert!(error.contains("default expression"), "{error}");
    }
}

#[test]
fn typescript_function_types_are_not_default_initializers() {
    let source = "function adjust(callback: () => number, value: number) { return callback() + value; }\nfunction choose() { return 1; }\nadjust(choose, 3);\n";
    let (dir, index) = workspace("a.ts", source);
    let plan = changed(&index, Change::Move { from: 0, to: 1 }).unwrap();
    let after = apply(dir.path(), "a.ts", &plan.edits);
    assert!(
        after.contains("adjust(value: number, callback: () => number)"),
        "{after}"
    );
    assert!(after.contains("adjust(3, choose)"), "{after}");
}

#[test]
fn typescript_literal_defaults_preserve_javascript_execution() {
    let source = include_str!("fixtures/signature/literal-defaults.ts");
    let (dir, index) = workspace("a.ts", source);
    let plan = changed(&index, Change::Move { from: 0, to: 2 }).unwrap();
    let after = apply(dir.path(), "a.ts", &plan.edits);
    let evaluate = |source: &str| {
        let result = Command::new("node").args(["-e", source]).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    };
    assert_eq!(evaluate(source), evaluate(&after));
}

#[test]
fn decorated_python_calling_conventions_require_explicit_migration() {
    let source =
        "def decorate(f):\n    return f\n@decorate\ndef adjust(unused):\n    return 1\nadjust(0)\n";
    let (_dir, index) = workspace("a.py", source);
    assert!(changed(&index, Change::Remove(0)).is_err());
}

#[test]
fn removing_a_commented_argument_keeps_a_valid_separator() {
    let source = "fn adjust(a: i32, /* unused */ b: i32, c: i32) -> i32 { a + c }\nfn run() { adjust(1, /* drop */ 2, 3); }\n";
    let (dir, index) = workspace("a.rs", source);
    let plan = changed(&index, Change::Remove(1)).unwrap();
    let after = apply(dir.path(), "a.rs", &plan.edits);
    assert!(after.contains("/* unused */"));
    assert!(after.contains("/* drop */"));
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict).unwrap();
}

#[test]
fn comments_before_separators_and_single_item_trailing_commas_survive_removal() {
    for source in [
        "fn adjust(a: i32, b: i32 /* keep */, c: i32) -> i32 { a + c }\nfn run() { adjust(1, 2 /* keep */, 3); }\n",
        "fn adjust(a: i32, /* keep */ b: i32) -> i32 { a }\nfn run() { adjust(1, /* keep */ 2); }\n",
        "fn adjust(b: i32,) {}\nfn run() { adjust(2,); }\n",
    ] {
        let (_dir, index) = workspace("a.rs", source);
        let position = if source.contains("a: i32") { 1 } else { 0 };
        let plan = changed(&index, Change::Remove(position)).unwrap();
        let outcomes = fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict).unwrap();
        assert!(!outcomes.is_empty());
    }
}

fn cli(root: &Path, args: &[&str]) -> (bool, serde_json::Value) {
    let result = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    let report = serde_json::from_slice(&result.stdout).unwrap_or_else(|_| {
        panic!(
            "{args:?}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        )
    });
    (result.status.success(), report)
}

fn accepted(root: &Path, args: &[&str]) -> serde_json::Value {
    let (success, report) = cli(root, args);
    assert!(success, "{args:?}: {report}");
    report
}

fn python_project(root: &Path) -> Vec<u8> {
    let result = Command::new("python3")
        .arg("run.py")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

#[test]
fn imported_signature_changes_preview_reverse_and_replay_in_another_checkout() {
    for (definition, call, change) in [
        (
            "def adjust(a, unused, c):\n    return a * 10 + c\n",
            "invoke(2, 0, c=3)",
            "remove:1",
        ),
        (
            "def adjust(a, b, c):\n    return a * 100 + b * 10 + c\n",
            "invoke(1, 2, 3)",
            "move:0:2",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let runner = format!("from api import adjust as invoke\nprint({call})\n");
        fs::write(root.path().join("api.py"), definition).unwrap();
        fs::write(root.path().join("run.py"), &runner).unwrap();
        let expected = python_project(root.path());
        accepted(root.path(), &["signature", "adjust", change]);
        assert_eq!(
            fs::read_to_string(root.path().join("api.py")).unwrap(),
            definition
        );
        assert!(!root.path().join(".fr-history").exists());
        let plan = accepted(root.path(), &["signature", "adjust", change, "--save-plan"]);
        let id = plan["transaction"].as_u64().unwrap().to_string();
        accepted(root.path(), &["history", "apply", &id, "--write"]);
        assert_eq!(python_project(root.path()), expected);
        let changed_definition = fs::read(root.path().join("api.py")).unwrap();
        let changed_runner = fs::read(root.path().join("run.py")).unwrap();
        accepted(root.path(), &["history", "undo", &id, "--write"]);
        assert_eq!(
            fs::read_to_string(root.path().join("api.py")).unwrap(),
            definition
        );
        assert_eq!(
            fs::read_to_string(root.path().join("run.py")).unwrap(),
            runner
        );
        accepted(root.path(), &["history", "redo", &id, "--write"]);
        assert_eq!(
            fs::read(root.path().join("api.py")).unwrap(),
            changed_definition
        );
        assert_eq!(
            fs::read(root.path().join("run.py")).unwrap(),
            changed_runner
        );
        let patch = root.path().join("migration.patch");
        accepted(
            root.path(),
            &["history", "patch", &id, "--output", patch.to_str().unwrap()],
        );
        let receiver = tempfile::tempdir().unwrap();
        fs::write(receiver.path().join("api.py"), definition).unwrap();
        fs::write(receiver.path().join("run.py"), runner).unwrap();
        let result = Command::new("git")
            .arg("apply")
            .arg(&patch)
            .current_dir(receiver.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(python_project(receiver.path()), expected);
        assert_eq!(
            fs::read(receiver.path().join("api.py")).unwrap(),
            changed_definition
        );
        assert_eq!(
            fs::read(receiver.path().join("run.py")).unwrap(),
            changed_runner
        );
    }
}

#[test]
fn a_competing_caller_edit_refuses_the_entire_saved_signature_plan() {
    let root = tempfile::tempdir().unwrap();
    let original = "def adjust(a, unused):\n    return a\n";
    fs::write(root.path().join("api.py"), original).unwrap();
    fs::write(
        root.path().join("run.py"),
        "from api import adjust\nprint(adjust(1, 0))\n",
    )
    .unwrap();
    let plan = accepted(
        root.path(),
        &["signature", "adjust", "remove:1", "--save-plan"],
    );
    let id = plan["transaction"].as_u64().unwrap().to_string();
    let competing = "from api import adjust\nprint(adjust(9, 0))\n";
    fs::write(root.path().join("run.py"), competing).unwrap();
    let (success, report) = cli(root.path(), &["history", "apply", &id, "--write"]);
    assert!(!success, "{report}");
    assert_eq!(
        fs::read_to_string(root.path().join("api.py")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(root.path().join("run.py")).unwrap(),
        competing
    );
}

#[test]
fn a_recipe_cannot_commit_an_earlier_rename_after_signature_refusal() {
    for argument in ["0", "record()"] {
        let root = tempfile::tempdir().unwrap();
        let source = format!("def record():\n    print('recorded')\n    return 0\ndef adjust(unused):\n    return 1\nprint(adjust({argument}))\n");
        fs::write(root.path().join("run.py"), &source).unwrap();
        let recipe = root.path().join("migration.recipe");
        fs::write(&recipe, "schema 1\nrecipe migrate {\n rename to \"updated\" where name=\"adjust\" kind=function\n signature \"remove:0\" where name=\"updated\" kind=function\n}\n").unwrap();
        let expected = python_project(root.path());
        let (success, report) = cli(
            root.path(),
            &["recipe", recipe.to_str().unwrap(), "--write"],
        );
        if argument == "record()" {
            assert!(!success, "{report}");
            assert_eq!(
                fs::read_to_string(root.path().join("run.py")).unwrap(),
                source
            );
            assert!(!root.path().join(".fr-history").exists());
        } else {
            assert!(success, "{report}");
            assert!(fs::read_to_string(root.path().join("run.py"))
                .unwrap()
                .contains("def updated():"));
        }
        assert_eq!(python_project(root.path()), expected);
    }
}

#[test]
fn python_parameter_kinds_and_literal_expansions_preserve_execution() {
    for (parameters, body, call, at) in [
        ("a, /, unused=0", "return a", "adjust(1, unused=0)", 1),
        ("unused, /, a", "return a", "adjust(0, 1)", 0),
        ("a, *, unused=0", "return a", "adjust(1, unused=0)", 1),
        ("*, unused=0", "return 1", "adjust(unused=0)", 0),
        ("a, *unused", "return a", "adjust(1, 0, 0)", 1),
        ("a, **unused", "return a", "adjust(1, extra=0)", 1),
        ("a, *unused, flag=0", "return a + flag", "adjust(1, 0, flag=2)", 1),
        ("unused, a", "return a", "adjust(*(0, 1))", 0),
        ("a, unused", "return a", "adjust(*[1, 0])", 1),
        ("a, *, unused=0", "return a", "adjust(1, **{'unused': 0})", 1),
    ] {
        let source = format!("def adjust({parameters}):\n    {body}\nprint({call})\n");
        let (dir,index) = workspace("subject.py", &source);
        let plan = changed(&index, Change::Remove(at)).unwrap();
        let after = apply(dir.path(), "subject.py", &plan.edits);
        assert_eq!(output("python3", &source), output("python3", &after), "{after}");
    }
}

#[test]
fn python_kind_aware_moves_keep_bindings_and_keyword_evaluation_order() {
    for (parameters, call, from, to) in [
        ("a, b, /", "adjust(1, 2)", 0, 1),
        ("*, a=1, b", "adjust(b=2)", 0, 1),
        ("a, *, b, c", "adjust(1, c=3, b=2)", 1, 2),
        ("a, b", "adjust(b=mark(2), a=mark(1))", 0, 1),
        ("a, b", "adjust(mark(1), b=mark(2))", 0, 1),
        ("a, b, *tail", "adjust(*(1, 2, 3))", 0, 1),
    ] {
        let source = format!("def mark(x):\n    print(x)\n    return x\ndef adjust({parameters}):\n    return a * 10 + b\nprint({call})\n");
        let (dir,index) = workspace("subject.py", &source);
        let plan = changed(&index, Change::Move { from,to }).unwrap();
        let after = apply(dir.path(), "subject.py", &plan.edits);
        assert_eq!(output("python3", &source), output("python3", &after), "{after}");
    }
}

#[test]
fn python_expanded_removal_keeps_comments_and_refuses_effects() {
    let source = "def adjust(unused, /, a):\n    return a\nprint(adjust(*(0, # keep this comment\n1)))\n";
    let (dir,index) = workspace("subject.py", source);
    let plan = changed(&index, Change::Remove(0)).unwrap();
    let after = apply(dir.path(), "subject.py", &plan.edits);
    assert!(after.contains("# keep this comment"));
    assert_eq!(output("python3", source), output("python3", &after));
    for call in ["adjust(*[mark(), 1])", "adjust(**{'unused': mark(), 'a': 1})", "adjust(*values)"] {
        let source = format!("def mark():\n    print('effect')\n    return 0\ndef adjust(unused, a):\n    return a\nvalues = [0, 1]\n{call}\n");
        let (_dir,index) = workspace("subject.py", &source);
        assert!(changed(&index, Change::Remove(0)).is_err());
    }
}
