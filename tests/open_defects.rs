//! The open entries in BUGS.md, held to what they say.
//!
//! Eight of the thirteen are limits of the published grammars, and
//! `tests/known_grammar_gaps.rs` pins every one of those from both sides. The rest are this
//! tool's own behaviour, and until now they were prose: a description of what happens, with
//! nothing to notice when it stopped happening. B11 said `@content` was a gap after it had
//! stopped being one, and nothing noticed for months.
//!
//! Each test here asserts the *whole* of its entry, both what the tool does not do and what it
//! does instead, because every one of these stands on the second half. An incomplete answer
//! that says so is a different thing from a wrong one. A test that checked only the
//! incompleteness would pass just as well if the report went away.
//!
//! A failure here means the entry is out of date. The entry is what to update.

use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn index_of(root: &Path) -> Index {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    Index::build_from_scan(&scanned).expect("index")
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

fn applied(root: &Path, file: &str, edits: &fun_refactor::edit::EditSet) -> String {
    let path = root.join(file);
    let before = std::fs::read_to_string(&path).expect("read");
    match edits.edits_for(&path) {
        Some(for_file) => {
            fun_refactor::edit::apply_to_string(&before, for_file).expect("the edits apply")
        }
        None => before,
    }
}

// ------------------------------------------------------------------- B286

#[test]
fn inline_brackets_by_the_value_and_not_by_the_destination() {
    // B286, and it is a decision and not an oversight. The bracket is needed when the use
    // site binds more tightly than the value does, and noise when it does not. The check would
    // have to be per-use-site and per-language. The two failure modes are not symmetric: an
    // extra bracket is noise, a missing one changes the arithmetic.
    //
    // Both halves are asserted, because the entry only stands while the needed bracket is still
    // there. A fix that dropped brackets everywhere would satisfy half of this and silently
    // change what code computes.
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "fn f(w: usize, h: usize) -> usize {\n    let base = w * 2 + h * 3;\n    \
         let scaled = base;\n    scaled\n}\n",
    )]);
    let index = index_of(&root);
    let plan =
        fun_refactor::refactor::inline::variable(&index, symbol(&index, "base")).expect("a plan");
    assert!(
        applied(&root, "a.rs", &plan.edits).contains("let scaled = (w * 2 + h * 3);"),
        "the noisy half of B286 is gone — update the entry:\n{}",
        applied(&root, "a.rs", &plan.edits)
    );

    let (_tmp2, root2) = workspace(&[(
        "b.rs",
        "fn f(w: usize, h: usize) -> usize {\n    let sum = w + h;\n    sum * 2\n}\n",
    )]);
    let index2 = index_of(&root2);
    let plan2 =
        fun_refactor::refactor::inline::variable(&index2, symbol(&index2, "sum")).expect("a plan");
    assert!(
        applied(&root2, "b.rs", &plan2.edits).contains("(w + h) * 2"),
        "the bracket that changes the arithmetic went missing:\n{}",
        applied(&root2, "b.rs", &plan2.edits)
    );
}

// --------------------------------------------------------------------- B5

#[test]
fn dispatch_is_followed_as_far_as_the_source_declares_it() {
    // B5. A call through a trait object resolves to no implementation, so reachability fans it
    // out to every type that declares itself an implementation. What is left is undecidable and
    // not unimplemented: a function held in a struct field and called through it is declared a
    // method of nothing. So there is no method set to look it up in.
    let (_tmp, root) = workspace(&[
        (
            "a.rs",
            "pub trait Shape {\n    fn area(&self) -> f64;\n}\npub struct Circle;\n\
             impl Shape for Circle {\n    fn area(&self) -> f64 {\n        1.0\n    }\n}\n\
             pub fn total(s: &dyn Shape) -> f64 {\n    s.area()\n}\n",
        ),
        (
            "b.rs",
            "pub struct Held {\n    pub run: fn() -> f64,\n}\n\
             pub fn go(h: &Held) -> f64 {\n    (h.run)()\n}\n\
             pub fn candidate() -> f64 {\n    2.0\n}\n",
        ),
    ]);
    let index = index_of(&root);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::detect(&index).expect("entry points");
    let dead: Vec<&str> = fun_refactor::refactor::delete::find_unused(&index, &entrypoints)
        .into_iter()
        .filter_map(|id| index.symbol(id))
        .map(|s| s.name.as_str())
        .collect();

    assert!(
        !dead.contains(&"area"),
        "the implementation is reached through the trait, so it is not dead: {dead:?}"
    );
    assert!(
        dead.contains(&"candidate"),
        "B5's remaining half is a function reached only through a struct field, and it is \
         still listed — if it no longer is, update the entry: {dead:?}"
    );
}

// -------------------------------------------------------------------- B14

#[test]
fn a_class_named_inside_a_helper_call_is_reported_and_not_rewritten() {
    // B14. Only a plain string attribute value is captured, so `cx("btn", …)` is not a
    // resolved use of the class. The rename is therefore incomplete, and it says so,
    // naming the file and position of every site it left, which keeps it from
    // being silently wrong.
    let (_tmp, root) = workspace(&[
        ("s.css", ".btn {\n  color: red;\n}\n"),
        (
            "c.tsx",
            "export function A() {\n  return <div className=\"btn\" />;\n}\n\
             export function B({ active }: { active: boolean }) {\n  \
             return <div className={cx(\"btn\", active && \"on\")} />;\n}\n\
             declare function cx(...parts: unknown[]): string;\n",
        ),
    ]);
    let index = index_of(&root);
    let plan = fun_refactor::refactor::rename::plan(&index, symbol(&index, "btn"), "primary")
        .expect("a plan");
    let out = applied(&root, "c.tsx", &plan.edits);

    assert!(
        out.contains("className=\"primary\""),
        "the plain attribute is the half that does work:\n{out}"
    );
    assert!(
        out.contains("cx(\"btn\""),
        "B14 says the helper call is left alone — if it is rewritten now, retire the \
         entry:\n{out}"
    );
    assert!(
        !plan.warnings.is_empty(),
        "an incomplete rename that reports nothing is silently wrong, which is the one \
         thing B14 says this is not"
    );
}

// -------------------------------------------------------------------- B13

#[test]
fn a_values_answer_names_the_channel_it_was_never_told_about() {
    // B13. Given some of the inputs and not others, the competition is decided *given the
    // inputs supplied*. The answer names the channel that was left out. Given none, nothing is
    // decided at all. Neither one infers an invocation.
    let (_tmp, root) = workspace(&[
        ("Chart.yaml", "name: chart\nversion: 0.1.0\n"),
        ("values.yaml", "replicas: 1\n"),
        (
            "templates/deploy.yaml",
            "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: {{ .Values.replicas }}\n",
        ),
    ]);
    let index = index_of(&root);
    let key = symbol(&index, "replicas");
    use fun_refactor::analysis::provenance::{self, ValuesInputs};

    // The report says this in its stops: a channel outside the workspace that could
    // pre-empt every source listed is a stop, and so is a competition the supplied inputs
    // settle. Both name what they were never told about.
    let said = |report: &provenance::Provenance| -> String {
        report
            .stops
            .iter()
            .map(|(_, reason)| reason.to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    };

    let nothing_supplied = ValuesInputs::parse(&[], &[], &[]).expect("no inputs");
    let without =
        provenance::provenance_with_inputs(&index, key, 5, &nothing_supplied).expect("a report");
    assert!(
        said(&without).contains("overridden externally"),
        "with no inputs the answer is undecided and says so: {}",
        said(&without)
    );

    let some_supplied =
        ValuesInputs::parse(&[], &["replicas=3".to_string()], &[]).expect("one --set");
    let with =
        provenance::provenance_with_inputs(&index, key, 5, &some_supplied).expect("a report");
    assert!(
        said(&with).contains("given the inputs supplied"),
        "with some inputs the answer is decided given them, and names what is missing: {}",
        said(&with)
    );
}

/// B364: a Zig file whose top level is fields, the file-as-struct idiom, loses them.
///
/// zls writes `const Self = @This();` and then fields at file scope. The reader has
/// no record to put them in, so each carries as unsupported. The entry is open
/// because the fix is a design: a record named after the file, from its fields.
#[test]
fn b364_zig_file_level_fields_carry_as_unsupported() {
    let (_tmp, root) = workspace(&[(
        "store.zig",
        "const Store = @This();\n\nio: i64,\nconfig: i64,\n\npub fn size(self: *Store) i64 {\n    return self.io;\n}\n",
    )]);
    let plan = fun_refactor::transpile::plan(
        &root.join("store.zig"),
        fun_refactor::lang::Language::TypeScript,
    )
    .expect("a draft");
    assert!(
        plan.output.contains("fun-refactor: not translated"),
        "the file-level fields translated; B364 is stale:\n{}",
        plan.output
    );
    assert_eq!(plan.fidelity.records, 0, "a record appeared; B364 is stale");
}

/// B365: a Zig tagged union, `union(enum)`, has no crossing.
///
/// The same shape as a Rust enum with payloads, a TypeScript discriminated union.
/// The reader carries it whole. Open because the crossing is a feature: variants
/// with payloads in the IR.
#[test]
fn b365_zig_tagged_union_carries_as_unsupported() {
    let (_tmp, root) = workspace(&[(
        "result.zig",
        "pub const Answer = union(enum) {\n    none: void,\n    value: i64,\n};\n",
    )]);
    let plan =
        fun_refactor::transpile::plan(&root.join("result.zig"), fun_refactor::lang::Language::Rust)
            .expect("a draft");
    assert!(
        plan.output.contains("fun-refactor: not translated"),
        "the tagged union translated; B365 is stale:\n{}",
        plan.output
    );
}
