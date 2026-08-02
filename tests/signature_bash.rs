//! Changing a shell function's signature.
//!
//! A shell function declares nothing, so there is no parameter list to edit. What
//! there is instead is a numbering: the body reads `$1`, `$2`, … and every call site
//! passes words positionally. A signature change has to rewrite both halves and keep
//! them agreeing, and these tests pin the exact text on both sides.
//!
//! The refusals get as much room as the successes, because most of what makes this
//! operation possible is knowing when it is not. `$@`, `shift` and an unquoted
//! expansion each break the correspondence between a syntactic word and a positional
//! parameter, and each has a test naming it.

use fun_refactor::{
    edit::{self, apply_to_string, Validation},
    index::Index,
    model::SymbolId,
    refactor::{
        signature::{self, Change, Subject},
        Refusal,
    },
    scan::{scan, ScanOptions},
};
use std::path::{Path, PathBuf};

struct Workspace {
    tmp: tempfile::TempDir,
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Workspace {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        Workspace { tmp }
    }

    fn index(&self) -> Index {
        let scanned = scan(self.tmp.path(), &ScanOptions::default()).unwrap();
        Index::build_from_scan(&scanned).unwrap()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }
}

/// The one symbol with this name, failing loudly if it is ambiguous.
fn symbol_id(index: &Index, name: &str) -> SymbolId {
    let found = index.find_symbols(name, None);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one '{name}', got {:?}",
        found.iter().map(|s| (&s.file, s.kind)).collect::<Vec<_>>()
    );
    found[0].id
}

/// What one file looks like after the plan, without writing anything.
fn applied(plan: &signature::SignaturePlan, path: &Path) -> String {
    let original = std::fs::read_to_string(path).unwrap_or_default();
    match plan.edits.edits_for(path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

/// Validate the plan by reparsing every touched file, then write it.
fn commit(plan: &signature::SignaturePlan) -> Vec<PathBuf> {
    let outcomes = edit::plan(&plan.edits, Validation::ReparseStrict)
        .expect("the change must survive a strict reparse");
    let changed = outcomes
        .iter()
        .filter(|o| o.changed())
        .map(|o| o.path.clone())
        .collect();
    edit::commit(&outcomes).unwrap();
    changed
}

fn error(result: anyhow::Result<signature::SignaturePlan>) -> String {
    match result {
        Ok(plan) => panic!(
            "expected a refusal, got a plan touching {} file(s)",
            plan.edits.file_count()
        ),
        Err(e) => e.to_string(),
    }
}

/// The same, insisting the error is a structured [`Refusal`] rather than a bail.
fn refusal(result: anyhow::Result<signature::SignaturePlan>) -> String {
    match result {
        Ok(_) => panic!("expected a refusal, got a plan"),
        Err(e) => {
            assert!(
                e.downcast_ref::<Refusal>().is_some(),
                "expected a structured refusal, got: {e}"
            );
            e.to_string()
        }
    }
}

// ===========================================================================
// Removing an argument.
// ===========================================================================

#[test]
fn removes_a_middle_argument_and_renumbers_what_follows() {
    let ws = Workspace::new(&[(
        "run.sh",
        "greet() {\n  echo \"$1\"\n  echo \"$3\"\n}\ngreet a b c\n",
    )]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "greet"), Change::Remove(1)).unwrap();

    assert_eq!(plan.subject_kind, Subject::ShellFunction);
    assert_eq!(plan.call_sites, 1);
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "greet() {\n  echo \"$1\"\n  echo \"$2\"\n}\ngreet a c\n"
    );
    commit(&plan);
}

#[test]
fn removes_the_first_argument() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$2\"\n}\nf a b\n")]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nf b\n"
    );
    commit(&plan);
}

#[test]
fn removes_the_last_argument_without_leaving_a_trailing_space() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a b\n")]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(1)).unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nf a\n"
    );
    commit(&plan);
}

#[test]
fn removes_the_only_argument() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo hi\n}\nf a\n")]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert_eq!(applied(&plan, &ws.path("run.sh")), "f() {\n  echo hi\n}\nf\n");
    commit(&plan);
}

#[test]
fn updates_call_sites_in_every_script_that_sources_the_definition() {
    let ws = Workspace::new(&[
        ("lib.sh", "greet() {\n  echo \"$2\"\n}\n"),
        (
            "app.sh",
            "#!/usr/bin/env bash\nsource ./lib.sh\ngreet one two\n",
        ),
        // Sourcing is transitive: this one never names lib.sh directly.
        ("deep.sh", "source ./app.sh\ngreet three four\n"),
    ]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "greet"), Change::Remove(0)).unwrap();
    assert_eq!(plan.call_sites, 2);

    let changed = commit(&plan);
    assert_eq!(changed.len(), 3, "the body and both callers: {changed:?}");
    assert_eq!(ws.read("lib.sh"), "greet() {\n  echo \"$1\"\n}\n");
    assert_eq!(
        ws.read("app.sh"),
        "#!/usr/bin/env bash\nsource ./lib.sh\ngreet two\n"
    );
    assert_eq!(ws.read("deep.sh"), "source ./app.sh\ngreet four\n");
}

#[test]
fn refuses_to_remove_a_parameter_the_body_still_reads() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$2\"\n}\nf a b\n")]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(1),
    ));
    assert!(message.contains("still reads $2"), "got: {message}");
}

#[test]
fn notes_a_call_site_with_nothing_at_that_position() {
    // Passing fewer arguments than the function reads is legal shell: the parameter
    // is simply unset. There is then nothing to remove, which is worth saying.
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a b\nf a\n")]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(1)).unwrap();
    assert!(
        plan.notes.iter().any(|n| n.contains("position 1")),
        "got: {:?}",
        plan.notes
    );
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nf a\nf a\n"
    );
}

// ===========================================================================
// Moving and adding.
// ===========================================================================

#[test]
fn moving_swaps_both_the_arguments_and_the_body_references() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$1 $2\"\n}\nf first second\n",
    )]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Move { from: 0, to: 1 },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$2 $1\"\n}\nf second first\n"
    );
    commit(&plan);
}

#[test]
fn adding_inserts_the_argument_and_shifts_the_body_up() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1 $2\"\n}\nf a b\n")]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 0,
            declaration: String::new(),
            argument: "\"--flag\"".into(),
        },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$2 $3\"\n}\nf \"--flag\" a b\n"
    );
    commit(&plan);
}

#[test]
fn adding_at_the_end_appends_to_each_call() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a\n")]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 1,
            declaration: String::new(),
            argument: "b".into(),
        },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nf a b\n"
    );
    commit(&plan);
}

#[test]
fn adding_to_a_call_that_passes_nothing_starts_the_argument_list() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo hi\n}\nf\n")]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 0,
            declaration: String::new(),
            argument: "x".into(),
        },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo hi\n}\nf x\n"
    );
    commit(&plan);
}

#[test]
fn a_declaration_is_reported_rather_than_written_anywhere() {
    // Bash has nothing to declare, so the field cannot be honoured — but silently
    // dropping text the caller supplied would be worse than saying so.
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a\n")]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 1,
            declaration: "verbose: bool".into(),
            argument: "1".into(),
        },
    )
    .unwrap();
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("declares no parameters") && n.contains("verbose: bool")),
        "got: {:?}",
        plan.notes
    );
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nf a 1\n"
    );
}

#[test]
fn refuses_to_add_a_parameter_with_no_word_to_pass() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a\n")]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 1,
            declaration: String::new(),
            argument: String::new(),
        },
    ));
    assert!(message.contains("needs a word to pass"), "got: {message}");
}

#[test]
fn refuses_to_add_past_the_end_of_a_call() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a\n")]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 4,
            declaration: String::new(),
            argument: "x".into(),
        },
    ));
    assert!(message.contains("would land at position"), "got: {message}");
}

#[test]
fn notes_a_call_that_cannot_be_reordered_for_want_of_positions() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1 $2\"\n}\nf a b\nf a\n")]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Move { from: 0, to: 1 },
    )
    .unwrap();
    assert!(
        plan.notes.iter().any(|n| n.contains("not both present")),
        "got: {:?}",
        plan.notes
    );
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$2 $1\"\n}\nf b a\nf a\n"
    );
}

// ===========================================================================
// The numbering itself.
// ===========================================================================

#[test]
fn braced_references_are_renumbered_in_place() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"${2}-${3}\"\n}\nf a b c\n",
    )]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"${1}-${2}\"\n}\nf b c\n"
    );
    commit(&plan);
}

#[test]
fn renumbering_past_nine_has_to_start_bracing() {
    // `$10` is not parameter 10 — the shell reads it as `${1}0` — so a reference
    // pushed past nine must gain braces or the rewrite would change its meaning.
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$9\"\n}\nf 1 2 3 4 5 6 7 8 9\n",
    )]);
    let index = ws.index();
    let plan = signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Add {
            at: 0,
            declaration: String::new(),
            argument: "0".into(),
        },
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"${10}\"\n}\nf 0 1 2 3 4 5 6 7 8 9\n"
    );
    commit(&plan);
}

#[test]
fn dollar_zero_is_the_script_name_and_never_renumbered() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$0: $2\"\n}\nf a b\n",
    )]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$0: $1\"\n}\nf b\n"
    );
    commit(&plan);
}

#[test]
fn refuses_a_multi_digit_unbraced_reference() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$12\"\n}\nf a b\n",
    )]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(0),
    ));
    assert!(message.contains("is not parameter 12"), "got: {message}");
    assert!(message.contains("${12}"), "got: {message}");
}

#[test]
fn notes_a_body_that_reads_the_parameter_count() {
    // `$#` stays correct as an expression and wrong as an intent: the count it
    // reports is one lower than the code below it was written for.
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$# $2\"\n}\nf a b\n",
    )]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert!(
        plan.notes.iter().any(|n| n.contains("$#")),
        "got: {:?}",
        plan.notes
    );
}

// ===========================================================================
// The shapes no renumbering can follow.
// ===========================================================================

#[test]
fn refuses_a_body_that_expands_the_whole_parameter_list() {
    for body in ["  echo \"$@\"", "  echo $*"] {
        let ws = Workspace::new(&[("run.sh", &format!("f() {{\n{body}\n}}\nf a b\n"))]);
        let index = ws.index();
        let message = error(signature::change(
            &index,
            symbol_id(&index, "f"),
            Change::Remove(0),
        ));
        assert!(
            message.contains("whole parameter list"),
            "body {body:?} got: {message}"
        );
    }
}

#[test]
fn refuses_a_body_that_shifts() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  local first=\"$1\"\n  shift\n}\nf a b\n",
    )]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(1),
    ));
    assert!(message.contains("calls `shift`"), "got: {message}");
}

#[test]
fn refuses_a_body_that_replaces_the_parameters_with_set() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  set -- x y\n  echo \"$2\"\n}\nf a b\n",
    )]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(0),
    ));
    assert!(message.contains("calls `set --`"), "got: {message}");
}

#[test]
fn refuses_a_body_holding_a_nested_function() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  inner() {\n    echo \"$1\"\n  }\n  inner \"$2\"\n}\nf a b\n",
    )]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(0),
    ));
    assert!(message.contains("nested function"), "got: {message}");
}

#[test]
fn refuses_a_recursive_call_whose_argument_is_also_renumbered() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$2\"\n  f \"$2\" x\n}\nf a b\n",
    )]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Move { from: 0, to: 1 },
    ));
    assert!(
        message.contains("rewritten twice"),
        "got: {message}"
    );
}

// ===========================================================================
// Call sites that are not one word per position.
// ===========================================================================

#[test]
fn refuses_a_call_that_passes_dollar_at_before_the_position_changed() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$3\"\n}\nouter() {\n  f \"$@\" b\n}\n",
    )]);
    let index = ws.index();
    let message = refusal(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(1),
    ));
    assert!(message.contains("whole parameter list"), "got: {message}");
    assert!(message.contains("only known at run time"), "got: {message}");
}

#[test]
fn a_splitting_word_after_the_change_is_fine() {
    // Everything after the removed position shifts down by one however many words it
    // becomes, so `"$@"` at the end costs nothing.
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$2\"\n}\nouter() {\n  f a \"$@\"\n}\n",
    )]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nouter() {\n  f \"$@\"\n}\n"
    );
    commit(&plan);
}

#[test]
fn refuses_an_unquoted_expansion_at_a_position_being_changed() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$2\"\n}\nx=1\nf $x b\n",
    )]);
    let index = ws.index();
    let message = refusal(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(0),
    ));
    assert!(message.contains("unquoted expansion"), "got: {message}");
}

#[test]
fn quoting_the_same_expansion_makes_it_one_argument() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$2\"\n}\nx=1\nf \"$x\" b\n",
    )]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "f() {\n  echo \"$1\"\n}\nx=1\nf b\n"
    );
    commit(&plan);
}

#[test]
fn refuses_an_unquoted_glob_at_a_position_being_changed() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$2\"\n}\nf *.txt b\n",
    )]);
    let index = ws.index();
    let message = refusal(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(0),
    ));
    assert!(message.contains("glob or brace expansion"), "got: {message}");
}

#[test]
fn refuses_an_unquoted_command_substitution_at_a_position_being_changed() {
    let ws = Workspace::new(&[(
        "run.sh",
        "f() {\n  echo \"$2\"\n}\nf $(date) b\n",
    )]);
    let index = ws.index();
    let message = refusal(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(0),
    ));
    assert!(
        message.contains("unquoted command substitution"),
        "got: {message}"
    );
}

// ===========================================================================
// Which calls are calls at all.
// ===========================================================================

#[test]
fn refuses_when_two_functions_share_the_name() {
    let ws = Workspace::new(&[
        ("a.sh", "f() {\n  echo \"$1\"\n}\n"),
        ("b.sh", "f() {\n  echo \"$1\"\n}\n"),
    ]);
    let index = ws.index();
    let id = index.find_symbols("f", Some(&ws.path("a.sh")))[0].id;
    let message = refusal(signature::change(&index, id, Change::Remove(0)));
    assert!(message.contains("already defined in"), "got: {message}");
}

#[test]
fn refuses_a_caller_that_sources_a_computed_path() {
    let ws = Workspace::new(&[
        ("lib.sh", "greet() {\n  echo \"$2\"\n}\n"),
        (
            "app.sh",
            "dir=.\nsource \"$dir/lib.sh\"\ngreet one two\n",
        ),
    ]);
    let index = ws.index();
    let message = refusal(signature::change(
        &index,
        symbol_id(&index, "greet"),
        Change::Remove(0),
    ));
    assert!(message.contains("not a literal"), "got: {message}");
}

#[test]
fn a_same_named_command_that_never_sources_the_definition_is_reported_not_edited() {
    let ws = Workspace::new(&[
        ("lib.sh", "greet() {\n  echo \"$2\"\n}\n"),
        ("other.sh", "greet one two\n"),
    ]);
    let index = ws.index();
    let plan =
        signature::change(&index, symbol_id(&index, "greet"), Change::Remove(0)).unwrap();
    assert_eq!(plan.call_sites, 0);
    assert!(
        plan.notes.iter().any(|n| n.contains("never sources")),
        "got: {:?}",
        plan.notes
    );
    let changed = commit(&plan);
    assert_eq!(changed, vec![ws.path("lib.sh")], "only the body changed");
    assert_eq!(ws.read("other.sh"), "greet one two\n");
}

#[test]
fn refuses_when_a_script_naming_it_does_not_parse() {
    // A call inside a syntax error produces no reference at all, so the call surface
    // cannot be shown to be complete.
    let ws = Workspace::new(&[
        ("lib.sh", "greet() {\n  echo \"$2\"\n}\ngreet a b\n"),
        ("broken.sh", "if [ ; then\n  greet a b\nfi\n"),
    ]);
    let index = ws.index();
    assert!(
        index
            .file(&ws.path("broken.sh"))
            .is_some_and(|f| f.had_parse_errors),
        "the sample must actually fail to parse for this test to mean anything"
    );
    let message = refusal(signature::change(
        &index,
        symbol_id(&index, "greet"),
        Change::Remove(0),
    ));
    assert!(message.contains("do not parse cleanly"), "got: {message}");
}

#[test]
fn refuses_a_symbol_that_is_not_a_function() {
    let ws = Workspace::new(&[("run.sh", "NAME=x\n")]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "NAME"),
        Change::Remove(0),
    ));
    assert!(
        message.contains("only a shell function has positional parameters"),
        "got: {message}"
    );
}

// ===========================================================================
// The result.
// ===========================================================================

#[test]
fn the_rewritten_workspace_still_resolves() {
    let ws = Workspace::new(&[
        ("lib.sh", "greet() {\n  echo \"$1 $2\"\n}\n"),
        ("app.sh", "source ./lib.sh\ngreet a b c\n"),
    ]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "greet"), Change::Remove(2)).unwrap();
    commit(&plan);

    assert_eq!(ws.read("lib.sh"), "greet() {\n  echo \"$1 $2\"\n}\n");
    assert_eq!(ws.read("app.sh"), "source ./lib.sh\ngreet a b\n");

    // Rebuilt from the result, the call still points at the function.
    let rebuilt = ws.index();
    let greet = symbol_id(&rebuilt, "greet");
    let calls: Vec<_> = rebuilt
        .references
        .iter()
        .filter(|r| r.name == "greet" && r.kind == fun_refactor::model::ReferenceKind::Call)
        .collect();
    assert_eq!(calls.len(), 1, "the call survives: {calls:?}");
    let _ = greet;
}

#[test]
fn untouched_bytes_survive_exactly() {
    let original = "#!/usr/bin/env bash\n# a comment   \nset -euo pipefail\n\nf() {\n  # keep\t this\n  echo \"$1\"   \n}\n\nf   a   b\n";
    let ws = Workspace::new(&[("run.sh", original)]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(1)).unwrap();
    // Only the removed word and the single space before it go.
    assert_eq!(
        applied(&plan, &ws.path("run.sh")),
        "#!/usr/bin/env bash\n# a comment   \nset -euo pipefail\n\nf() {\n  # keep\t this\n  echo \"$1\"   \n}\n\nf   a\n"
    );
    commit(&plan);
}

#[test]
fn refuses_a_change_that_would_rewrite_nothing() {
    // Position 4 exists nowhere: no call passes it and the body never reads `$5`. A
    // plan with no edits would look like success.
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$1\"\n}\nf a\n")]);
    let index = ws.index();
    let message = error(signature::change(
        &index,
        symbol_id(&index, "f"),
        Change::Remove(4),
    ));
    assert!(
        message.contains("leaves `f` exactly as it was"),
        "got: {message}"
    );
    assert!(message.contains("position 4"), "got: {message}");
}

#[test]
fn the_summary_names_the_positional_parameter() {
    let ws = Workspace::new(&[("run.sh", "f() {\n  echo \"$2\"\n}\nf a b\n")]);
    let index = ws.index();
    let plan = signature::change(&index, symbol_id(&index, "f"), Change::Remove(0)).unwrap();
    let summary = signature::describe(&index, &plan);
    assert!(
        summary.contains("removed positional parameter 0"),
        "got: {summary}"
    );
    assert!(summary.contains("1 call site(s)"), "got: {summary}");
}
