//! Cascading flag removal, end to end.
//!
//! The point of this refactoring is the chain, not the first edit: uses become
//! constants, conditionals collapse, and whatever only the dead branch called becomes
//! unused. These tests check the whole chain runs to a fixpoint.

use fun_refactor::edit::apply_to_string;
use fun_refactor::refactor::cascade;
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    tmp
}

fn result_for(tmp: &Path, file: &str, plan: &cascade::CascadePlan) -> String {
    let path = tmp.join(file);
    let original = std::fs::read_to_string(&path).unwrap();
    match plan.edits.edits_for(&path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

#[test]
fn removing_a_true_flag_keeps_the_enabled_branch() {
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn run() {\n    if USE_NEW {\n        new_path();\n    } else {\n        old_path();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.rs", &plan);

    assert!(out.contains("new_path();"), "got:\n{out}");
    assert!(
        !out.contains("old_path();"),
        "dead branch should go:\n{out}"
    );
    assert!(!out.contains("USE_NEW"), "the flag should be gone:\n{out}");
    assert!(!out.contains("if true"), "the test should collapse:\n{out}");
}

#[test]
fn removing_a_false_flag_keeps_the_other_branch() {
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn run() {\n    if USE_NEW {\n        new_path();\n    } else {\n        old_path();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", false).unwrap();
    let out = result_for(tmp.path(), "a.rs", &plan);

    assert!(out.contains("old_path();"), "got:\n{out}");
    assert!(!out.contains("new_path();"), "got:\n{out}");
}

#[test]
fn the_cascade_removes_what_the_dead_branch_alone_used() {
    // `only_old` is called only from the branch being deleted, so it should go too.
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn only_old() {}\n\nfn run() {\n    if USE_NEW {\n        keep();\n    } else {\n        only_old();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.rs", &plan);

    assert!(out.contains("keep();"), "got:\n{out}");
    assert!(
        !out.contains("fn only_old"),
        "the now-unused function should be removed too:\n{out}"
    );
    assert!(
        plan.rounds.len() >= 2,
        "expected a cascade: {:?}",
        plan.rounds
    );
}

#[test]
fn a_function_still_used_elsewhere_survives() {
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn shared() {}\n\nfn other() { shared(); }\n\nfn run() {\n    if USE_NEW {\n        keep();\n    } else {\n        shared();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.rs", &plan);
    assert!(
        out.contains("fn shared"),
        "it still has a live caller:\n{out}"
    );
}

#[test]
fn a_flag_with_no_conditional_just_disappears() {
    let tmp = workspace(&[(
        "a.rs",
        "const FLAG: bool = true;\n\nfn run() {\n    log(FLAG);\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "FLAG", true).unwrap();
    let out = result_for(tmp.path(), "a.rs", &plan);
    assert!(out.contains("log(true);"), "got:\n{out}");
    assert!(!out.contains("FLAG"), "got:\n{out}");
}

#[test]
fn works_across_files() {
    let tmp = workspace(&[
        ("flags.rs", "pub const USE_NEW: bool = true;\n"),
        (
            "app.rs",
            "use flags::USE_NEW;\n\nfn run() {\n    if USE_NEW {\n        new_path();\n    } else {\n        old_path();\n    }\n}\n",
        ),
    ]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let app = result_for(tmp.path(), "app.rs", &plan);
    assert!(app.contains("new_path();"), "got:\n{app}");
    assert!(!app.contains("old_path();"), "got:\n{app}");
}

#[test]
fn works_for_python_with_python_spelling() {
    let tmp = workspace(&[(
        "a.py",
        "USE_NEW = True\n\ndef run():\n    if USE_NEW:\n        new_path()\n    else:\n        old_path()\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.py", &plan);
    assert!(out.contains("new_path()"), "got:\n{out}");
    assert!(!out.contains("old_path()"), "got:\n{out}");
}

#[test]
fn an_unknown_flag_is_an_error_rather_than_a_silent_no_op() {
    let tmp = workspace(&[("a.rs", "fn run() {}\n")]);
    let err = cascade::remove_flag(tmp.path(), "NOT_THERE", true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no symbol named"), "got: {err}");
}

#[test]
fn the_result_still_parses() {
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn run() {\n    if USE_NEW {\n        new_path();\n    } else {\n        old_path();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("a cascade must not leave the file broken");
    assert!(!outcomes.is_empty());
}

#[test]
fn deleted_definitions_leave_no_blank_debris() {
    // A multi-line definition must be removed lines and all, together with the blank
    // line that separated it, otherwise a cascade leaves a widening gap.
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn only_old() {\n    helper();\n}\n\nfn run() {\n    if USE_NEW {\n        keep();\n    } else {\n        only_old();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    let out = result_for(tmp.path(), "a.rs", &plan);

    assert!(!out.starts_with('\n'), "leading blank lines:\n{out:?}");
    assert!(
        !out.contains("\n\n\n"),
        "runs of blank lines left behind:\n{out}"
    );
    assert!(out.trim_start().starts_with("fn run"), "got:\n{out}");
}

#[test]
fn rounds_are_reported_so_the_cascade_is_visible() {
    let tmp = workspace(&[(
        "a.rs",
        "const USE_NEW: bool = true;\n\nfn run() {\n    if USE_NEW {\n        keep();\n    }\n}\n",
    )]);

    let plan = cascade::remove_flag(tmp.path(), "USE_NEW", true).unwrap();
    assert!(!plan.rounds.is_empty());
    assert!(plan.rounds[0].description.contains("USE_NEW"));
    assert!(plan.rounds.iter().all(|r| r.files_touched > 0));
}
