//! Safe delete: what it removes, and — more importantly — what it refuses to remove.

use fun_refactor::{
    edit::apply_to_string,
    index::Index,
    model::SymbolId,
    refactor::{delete, WarningKind},
    scan::{scan, ScanOptions},
};
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

fn only_symbol(index: &Index, name: &str) -> SymbolId {
    let found = index.find_symbols(name, None);
    assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
    found[0].id
}

fn applied(index: &Index, plan: &delete::DeletePlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    let _ = index;
    match plan.edits.edits_for(path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

// ------------------------------------------------------------------- refusals

#[test]
fn refuses_while_a_reference_still_resolves_to_the_symbol() {
    let source = "fn helper() {}\nfn main() {\n    helper();\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let error = delete::plan(&index, only_symbol(&index, "helper")).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("refusing to delete 'helper'"),
        "got: {message}"
    );
    assert!(message.contains("1 reference(s)"), "got: {message}");
}

#[test]
fn the_refusal_names_every_blocking_site_with_line_and_column() {
    let source = "fn helper() {}\nfn a() { helper(); }\nfn b() {\n    helper();\n}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let message = delete::plan(&index, only_symbol(&index, "helper"))
        .unwrap_err()
        .to_string();

    let path = tmp.path().join("a.rs");
    // `helper()` sits at column 10 on line 2 and column 5 on line 4.
    assert!(
        message.contains(&format!("{}:2:10", path.display())),
        "got: {message}"
    );
    assert!(
        message.contains(&format!("{}:4:5", path.display())),
        "got: {message}"
    );
    assert!(message.contains("2 reference(s)"), "got: {message}");
}

#[test]
fn refuses_across_files_and_across_languages() {
    // The HTML `class="btn"` is a use of the CSS class, and must block its deletion.
    let (tmp, index) = workspace(&[
        ("style.css", ".btn { color: red; }\n"),
        ("page.html", "<div class=\"btn\">hi</div>\n"),
    ]);

    let message = delete::plan(&index, only_symbol(&index, "btn"))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains(&tmp.path().join("page.html").display().to_string()),
        "the HTML use must be named: {message}"
    );
}

#[test]
fn a_reference_the_index_could_not_prove_does_not_block_but_is_reported() {
    // Two files each define `parse`; the call in `b.rs` is ambiguous, so it resolves
    // only weakly. That is not proof of a use, and must not silently block or be
    // silently ignored.
    let (_tmp, index) = workspace(&[
        ("a.rs", "pub fn shared_thing() {}\n"),
        ("b.rs", "fn other() { shared_thing(); }\n"),
    ]);

    let target = only_symbol(&index, "shared_thing");
    let weak = index
        .references_to(target)
        .iter()
        .all(|r| !r.confidence.is_safe_to_rewrite());
    assert!(weak, "this fixture is only interesting if resolution is weak");

    let plan = delete::plan(&index, target).expect("a weak reference must not block");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.kind == WarningKind::WeaklyResolved),
        "the weak reference must be reported: {:?}",
        plan.warnings
    );
}

#[test]
fn a_recursive_call_inside_the_definition_does_not_block_its_own_deletion() {
    let source = "fn loops() {\n    loops();\n}\nfn main() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "loops"))
        .expect("a self-call is not an outside use");
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "fn main() {}\n"
    );
}

#[test]
fn an_unknown_symbol_is_an_error_not_a_no_op() {
    let (_tmp, index) = workspace(&[("a.rs", "fn a() {}\n")]);
    let error = delete::plan(&index, SymbolId(9999)).unwrap_err();
    assert!(error.to_string().contains("unknown symbol"), "{error}");
}

// -------------------------------------------------------------------- deleting

#[test]
fn deletes_an_unused_definition_whole_line_and_all() {
    let source = "fn unused() {}\nfn main() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "unused")).unwrap();
    assert_eq!(plan.sites, 1);
    assert_eq!(plan.edits.edit_count(), 1);
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "fn main() {}\n"
    );
}

#[test]
fn everything_around_the_deleted_definition_survives_byte_for_byte() {
    let source = "// keep   this comment\nfn   gone( ) {\n    let x = 1;\n}\n\nfn keeper() {\n    // spacing   preserved\n}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "gone")).unwrap();
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "// keep   this comment\n\nfn keeper() {\n    // spacing   preserved\n}\n"
    );
}

#[test]
fn deleting_the_first_definition_does_not_leave_the_file_starting_blank() {
    let source = "fn first() {}\n\nfn second() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "first")).unwrap();
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "fn second() {}\n"
    );
}

#[test]
fn deleting_a_middle_definition_does_not_double_the_blank_lines() {
    let source = "fn a() {}\n\nfn b() {}\n\nfn c() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "b")).unwrap();
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "fn a() {}\n\nfn c() {}\n"
    );
}

#[test]
fn deletes_in_python_and_go_too() {
    let python = "def py_gone():\n    pass\n\n\ndef stays():\n    pass\n";
    let go = "package main\n\nfunc goGone() {}\n\nfunc main() {}\n";
    let (tmp, index) = workspace(&[("a.py", python), ("b.go", go)]);

    let py_plan = delete::plan(&index, only_symbol(&index, "py_gone")).unwrap();
    assert_eq!(
        applied(&index, &py_plan, &tmp.path().join("a.py")),
        "\ndef stays():\n    pass\n",
        "one blank line of the two-line PEP-8 gap is swallowed, not both"
    );

    let go_plan = delete::plan(&index, only_symbol(&index, "goGone")).unwrap();
    assert_eq!(
        applied(&index, &go_plan, &tmp.path().join("b.go")),
        "package main\n\nfunc main() {}\n"
    );
}

#[test]
fn a_symbol_referenced_only_from_a_string_is_deleted_but_the_string_is_reported() {
    let source = "fn handler() {}\nfn main() {\n    dispatch(\"handler\");\n}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "handler")).unwrap();
    let textual: Vec<_> = plan
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::TextualOccurrence)
        .collect();
    assert_eq!(textual.len(), 1, "got {textual:?}");
    assert_eq!(textual[0].line, 3);

    // The delete still happens — the string is reported, not obeyed.
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "fn main() {\n    dispatch(\"handler\");\n}\n"
    );
}

#[test]
fn files_that_failed_to_parse_are_reported_as_possibly_hiding_uses() {
    let (_tmp, index) = workspace(&[("a.rs", "fn alone() {}\n"), ("broken.rs", "fn oops( {\n")]);
    let plan = delete::plan(&index, only_symbol(&index, "alone")).unwrap();
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.kind == WarningKind::ParseErrors),
        "got {:?}",
        plan.warnings
    );
}

#[test]
fn deleting_a_css_selector_removes_only_the_selector_bytes() {
    // Documented behaviour, not desired behaviour: a CSS selector's `full_span` is the
    // selector node (`.btn`), never the rule set it heads, so the delete leaves the
    // declaration block behind. Fixing this belongs in queries/css/facts.scm.
    let source = ".btn { color: red; }\n";
    let (tmp, index) = workspace(&[("style.css", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "btn")).unwrap();
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("style.css")),
        " { color: red; }\n",
        "the orphaned declaration block is the known gap"
    );
}

// ------------------------------------------------------------- unused symbols

#[test]
fn find_unused_reports_an_orphan_and_not_the_entry_point() {
    let source = "fn used() {}\nfn orphan() {}\nfn main() {\n    used();\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let main = only_symbol(&index, "main");
    let unused = delete::find_unused(&index, &[main]);

    assert!(unused.contains(&only_symbol(&index, "orphan")));
    assert!(!unused.contains(&main), "the entry point is reachable");
    assert!(
        !unused.contains(&only_symbol(&index, "used")),
        "reachable from the entry point"
    );
}

#[test]
fn find_unused_finds_dead_recursive_code() {
    // `dead` only ever calls itself, so its single incoming reference is its own.
    let source = "fn dead() {\n    dead();\n}\nfn main() {}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(unused.contains(&only_symbol(&index, "dead")), "got {unused:?}");
}

#[test]
fn find_unused_cannot_see_mutual_recursion_between_dead_functions() {
    // The honest counterpart of the test above: `ping` and `pong` reference each
    // other, so neither has zero incoming references and neither is reported, even
    // though the pair is unreachable. This is a documented blind spot.
    let source = "fn ping() { pong(); }\nfn pong() { ping(); }\nfn main() {}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(!unused.contains(&only_symbol(&index, "ping")), "got {unused:?}");
    assert!(!unused.contains(&only_symbol(&index, "pong")), "got {unused:?}");
}

#[test]
fn find_unused_lists_live_code_reached_only_by_dynamic_dispatch() {
    // `on_event` is called through a name-keyed handler table the index cannot see —
    // the only mention of it is a string literal. Reachability follows resolved edges
    // only, so this live function is reported as unused. The list is a review list.
    let source = "fn on_event() {}\nfn main() {\n    dispatch(\"on_event\");\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(
        unused.contains(&only_symbol(&index, "on_event")),
        "the false positive is the documented behaviour: {unused:?}"
    );
}

#[test]
fn find_unused_finds_a_css_class_no_markup_uses() {
    let (_tmp, index) = workspace(&[
        ("style.css", ".used { color: red; }\n.dead { color: blue; }\n"),
        ("page.html", "<div class=\"used\">hi</div>\n"),
    ]);

    let unused = delete::find_unused(&index, &[]);
    assert!(unused.contains(&only_symbol(&index, "dead")), "got {unused:?}");
    assert!(!unused.contains(&only_symbol(&index, "used")));
}

#[test]
fn without_entry_points_reachability_contributes_nothing() {
    let source = "fn used() {}\nfn main() {\n    used();\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(&index, &[]);
    assert!(
        unused.contains(&only_symbol(&index, "main")),
        "nothing references main and no entry point was given: {unused:?}"
    );
    assert!(
        !unused.contains(&only_symbol(&index, "used")),
        "used still has an incoming reference"
    );
}

#[test]
fn an_unused_symbol_from_find_unused_can_then_be_deleted() {
    // The two halves must agree: what `find_unused` reports, `plan` must accept.
    let source = "fn orphan() {}\nfn main() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    let orphan = only_symbol(&index, "orphan");
    assert!(unused.contains(&orphan));

    let plan = delete::plan(&index, orphan).unwrap();
    assert_eq!(
        applied(&index, &plan, &tmp.path().join("a.rs")),
        "fn main() {}\n"
    );
}
