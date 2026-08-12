//! Safe delete: what it removes, and, more importantly, what it refuses to remove.

use fun_refactor::analysis::entrypoints::Entrypoints;
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

/// Apply a plan's edits to one file and return the resulting text.
fn applied(plan: &delete::DeletePlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
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
    assert!(
        weak,
        "this fixture is only interesting if resolution is weak"
    );

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
    assert_eq!(applied(&plan, &tmp.path().join("a.rs")), "fn main() {}\n");
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
    assert_eq!(applied(&plan, &tmp.path().join("a.rs")), "fn main() {}\n");
}

#[test]
fn everything_around_the_deleted_definition_survives_byte_for_byte() {
    let source = "// keep   this comment\nfn   gone( ) {\n    let x = 1;\n}\n\nfn keeper() {\n    // spacing   preserved\n}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "gone")).unwrap();
    assert_eq!(
        applied(&plan, &tmp.path().join("a.rs")),
        "// keep   this comment\n\nfn keeper() {\n    // spacing   preserved\n}\n"
    );
}

#[test]
fn deleting_the_first_definition_does_not_leave_the_file_starting_blank() {
    let source = "fn first() {}\n\nfn second() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "first")).unwrap();
    assert_eq!(applied(&plan, &tmp.path().join("a.rs")), "fn second() {}\n");
}

#[test]
fn deleting_a_middle_definition_does_not_double_the_blank_lines() {
    let source = "fn a() {}\n\nfn b() {}\n\nfn c() {}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "b")).unwrap();
    assert_eq!(
        applied(&plan, &tmp.path().join("a.rs")),
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
        applied(&py_plan, &tmp.path().join("a.py")),
        "\ndef stays():\n    pass\n",
        "one blank line of the two-line PEP-8 gap is swallowed, not both"
    );

    let go_plan = delete::plan(&index, only_symbol(&index, "goGone")).unwrap();
    assert_eq!(
        applied(&go_plan, &tmp.path().join("b.go")),
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

    // The delete still happens, the string is reported, not obeyed.
    assert_eq!(
        applied(&plan, &tmp.path().join("a.rs")),
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
            .any(|w| w.kind == WarningKind::IncompleteFacts),
        "got {:?}",
        plan.warnings
    );
}

#[test]
fn deleting_a_lone_css_selector_removes_its_whole_rule() {
    // A selector's `full_span` is the selector node, which is what a rename rewrites.
    // A delete has to take the rule too: a declaration block with nothing to apply to
    // is not valid CSS.
    let source = ".btn { color: red; }\n.other { padding: 1px; }\n";
    let (tmp, index) = workspace(&[("style.css", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "btn")).unwrap();
    assert_eq!(
        applied(&plan, &tmp.path().join("style.css")),
        ".other { padding: 1px; }\n"
    );
}

#[test]
fn deleting_one_of_several_selectors_leaves_the_rule_standing() {
    // The rule still applies to its remaining selectors, so only the named one and
    // the comma joining it are removed.
    let source = ".card, .btn, .wide { margin: 0; }\n";
    let (tmp, index) = workspace(&[("style.css", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "btn")).unwrap();
    assert_eq!(
        applied(&plan, &tmp.path().join("style.css")),
        ".card, .wide { margin: 0; }\n"
    );
}

#[test]
fn deleting_the_last_selector_in_a_list_takes_the_preceding_comma() {
    let source = ".card, .btn { margin: 0; }\n";
    let (tmp, index) = workspace(&[("style.css", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "btn")).unwrap();
    assert_eq!(
        applied(&plan, &tmp.path().join("style.css")),
        ".card { margin: 0; }\n"
    );
}

// ------------------------------------------------------------- unused symbols

#[test]
fn find_unused_reports_an_orphan_and_not_the_entry_point() {
    let source = "fn used() {}\nfn orphan() {}\nfn main() {\n    used();\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let main = only_symbol(&index, "main");
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[main]));

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

    let unused = delete::find_unused(
        &index,
        &Entrypoints::exactly(&[only_symbol(&index, "main")]),
    );
    assert!(
        unused.contains(&only_symbol(&index, "dead")),
        "got {unused:?}"
    );
}

#[test]
fn find_unused_reports_mutual_recursion_as_a_dead_group() {
    // `ping` and `pong` reference each other, so neither has zero incoming references
    // and the per-symbol check clears both. Asking the question of the cycle instead,
    // does anything *outside* it reference a member?, finds the whole component dead.
    let source = "fn ping() { pong(); }\nfn pong() { ping(); }\nfn main() {}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(
        &index,
        &Entrypoints::exactly(&[only_symbol(&index, "main")]),
    );
    assert!(
        unused.contains(&only_symbol(&index, "ping")),
        "got {unused:?}"
    );
    assert!(
        unused.contains(&only_symbol(&index, "pong")),
        "got {unused:?}"
    );
}

#[test]
fn find_unused_leaves_out_a_name_a_string_literal_spells() {
    // `on_event` is called through a name-keyed handler table the index cannot see.
    // Reachability follows resolved edges only, so nothing leads to it, but the
    // string is evidence that something might, and a candidate list that invites
    // deleting live code is worse than one with a stale entry missing from it.
    let source = "fn on_event() {}\nfn main() {\n    dispatch(\"on_event\");\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(
        &index,
        &Entrypoints::exactly(&[only_symbol(&index, "main")]),
    );
    assert!(
        !unused.contains(&only_symbol(&index, "on_event")),
        "a name spelled in a string may be reached by reflection: {unused:?}"
    );
}

#[test]
fn find_unused_finds_a_css_class_no_markup_uses() {
    let (_tmp, index) = workspace(&[
        (
            "style.css",
            ".used { color: red; }\n.dead { color: blue; }\n",
        ),
        ("page.html", "<div class=\"used\">hi</div>\n"),
    ]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    assert!(
        unused.contains(&only_symbol(&index, "dead")),
        "got {unused:?}"
    );
    assert!(!unused.contains(&only_symbol(&index, "used")));
}

#[test]
fn without_entry_points_reachability_contributes_nothing() {
    let source = "fn used() {}\nfn main() {\n    used();\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
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

    let unused = delete::find_unused(
        &index,
        &Entrypoints::exactly(&[only_symbol(&index, "main")]),
    );
    let orphan = only_symbol(&index, "orphan");
    assert!(unused.contains(&orphan));

    let plan = delete::plan(&index, orphan).unwrap();
    assert_eq!(applied(&plan, &tmp.path().join("a.rs")), "fn main() {}\n");
}

#[test]
fn the_edits_survive_the_engines_reparse_check() {
    // A plan is only useful if `edit::plan` will accept it: the file must still parse.
    let source = "fn orphan() {}\n\nfn main() {\n    let x = 1;\n}\n";
    let (tmp, index) = workspace(&[("a.rs", source)]);

    let plan = delete::plan(&index, only_symbol(&index, "orphan")).unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("deleting an unused function must not break the file");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].path, tmp.path().join("a.rs"));
    assert_eq!(outcomes[0].updated, "fn main() {\n    let x = 1;\n}\n");
}

#[test]
fn deleting_a_css_selector_survives_the_reparse_check() {
    // What is left has to still be CSS.
    let (_tmp, index) = workspace(&[(
        "style.css",
        ".btn { color: red; }\n.card, .btn { margin: 0; }\n",
    )]);
    // A CSS class has no canonical definition, so `.btn` here is two sites; deleting
    // the entity removes both.
    let first = index.find_symbols("btn", None)[0].id;
    let plan = delete::plan(&index, first).unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("the result must still parse");
    assert_eq!(outcomes.len(), 1);
    assert!(
        !outcomes[0].updated.contains("btn"),
        "got:\n{}",
        outcomes[0].updated
    );
}

#[test]
fn find_unused_leaves_out_a_name_the_author_marked_unused() {
    // A parameter a signature forces on you and the body ignores is written with a
    // leading underscore in Rust, TypeScript, Python and Zig. Listing those buries
    // the real findings: one real TypeScript file contributed eight of them.
    let source = "fn handler(_theme: i32, value: i32) -> i32 {\n    value\n}\n\
                  fn main() {\n    handler(1, 2);\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let unused = delete::find_unused(
        &index,
        &Entrypoints::exactly(&[only_symbol(&index, "main")]),
    );
    let named: Vec<String> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect();
    assert!(
        !named.iter().any(|n| n == "_theme"),
        "an underscore says the author meant it: {named:?}"
    );
}

#[test]
fn the_underscore_convention_is_reported_as_a_reason_not_hidden() {
    let source = "fn handler(_theme: i32, value: i32) -> i32 {\n    value\n}\n\
                  fn main() {\n    handler(1, 2);\n}\n";
    let (_tmp, index) = workspace(&[("a.rs", source)]);

    let report = delete::find_unused_report(
        &index,
        &Entrypoints::exactly(&[only_symbol(&index, "main")]),
    );
    let theme = index
        .symbols
        .iter()
        .find(|s| s.name == "_theme")
        .expect("the parameter is still in the index");
    let reason = report
        .explain(&index, theme.id)
        .expect("a spared symbol must say why it was spared");
    assert!(reason.contains("underscore"), "got: {reason}");
}

#[test]
fn an_exported_symbol_is_reported_but_marked_as_such() {
    // The distinction the `--internal` flag and the `exported` column rest on: a
    // library's public API has no caller in its own repository, and that is not
    // evidence of anything.
    let source = "package p\n\nfunc Exported() int {\n\treturn 1\n}\n\
                  func unexported() int {\n\treturn 2\n}\n";
    let (_tmp, index) = workspace(&[("a.go", source)]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let named: Vec<(&str, bool)> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| (s.name.as_str(), s.exported))
        .collect();
    assert!(named.contains(&("Exported", true)), "got {named:?}");
    assert!(named.contains(&("unexported", false)), "got {named:?}");
}

// ------------------------------------------- the two halves agree, in every language
//
// `an_unused_symbol_from_find_unused_can_then_be_deleted` above states the invariant
// and checks it for one Rust function. Run over a polyglot workspace it failed
// thirteen times out of fifty-nine: a TypeScript `export const` whose declarator span
// left `export const ;` behind, a Zig struct field, and nine CSS selectors that were
// not dead at all, the markup used them, and the use resolved to one of the two
// stylesheets that declared them, so the other read as unreferenced.
//
// One symbol was never going to find that. This runs the whole loop.

/// A little service in nine languages: enough shapes for the invariant to bite.
fn polyglot() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/lib.rs",
            "pub fn kept() -> i32 {\n    7\n}\n\npub fn orphan() -> i32 {\n    1\n}\n\nfn main() {\n    let _ = kept();\n}\n",
        ),
        (
            "src/types.zig",
            "pub const Reading = struct {\n    sensor: []const u8,\n    at: u64,\n};\n\npub fn use(r: Reading) usize {\n    return r.sensor.len;\n}\n",
        ),
        (
            "cmd/serve.go",
            "package cmd\n\ntype Config struct {\n\tHost string\n\tPort int\n}\n\nfunc Serve(c Config) string {\n\treturn c.Host\n}\n\nfunc unusedHelper() int {\n\treturn 1\n}\n",
        ),
        (
            "web/app.ts",
            "export const limits = { min: 0, max: 1 };\nexport const unusedConst = 42;\n\nexport function kept(): number {\n  return limits.max;\n}\n\nfunction orphanTs(): number {\n  return 2;\n}\n",
        ),
        (
            "web/Panel.tsx",
            "export function Panel() {\n  return <div className=\"panel\">hi</div>;\n}\n\nfunction Unused() {\n  return <span className=\"gone\">x</span>;\n}\n",
        ),
        (
            "scripts/run.py",
            "CONSTANT = 1\nUNUSED_CONSTANT = 2\n\n\ndef kept():\n    return CONSTANT\n\n\ndef orphan_py():\n    return 3\n",
        ),
        (
            "web/index.html",
            "<!doctype html>\n<html><body>\n<a class=\"panel\">one</a>\n<p class=\"note\" id=\"here\">two</p>\n</body></html>\n",
        ),
        (
            // `panel` and `note` are declared twice over, and used by the markup. A
            // per-site count called the second declaration of each dead.
            "web/base.css",
            ".panel {\n  color: red;\n}\n\n.note {\n  color: blue;\n}\n\n.never-used {\n  color: green;\n}\n",
        ),
        (
            "web/theme.css",
            ".panel {\n  background: black;\n}\n\n.note, .also-never {\n  background: grey;\n}\n",
        ),
        (
            "scripts/deploy.sh",
            "#!/usr/bin/env bash\nkept() {\n  echo hi\n}\n\norphan_sh() {\n  echo bye\n}\n\nkept\n",
        ),
    ]
}

#[test]
fn what_find_unused_reports_delete_can_always_remove() {
    let files = polyglot();
    let (_tmp, index) = workspace(&files);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    assert!(
        unused.len() > 8,
        "the fixture should produce plenty of candidates, got {}",
        unused.len()
    );

    let mut refused = Vec::new();
    let mut checked = 0;
    for id in &unused {
        // `find_unused` just returned this id, so the index not knowing it is a
        // contradiction and not a reason to look at one candidate fewer.
        let symbol = index
            .symbol(*id)
            .unwrap_or_else(|| panic!("find_unused returned an id the index does not hold"));
        checked += 1;
        match delete::plan(&index, *id) {
            Ok(plan) => assert!(
                !plan.edits.is_empty(),
                "{} {} produced a plan that changes nothing",
                symbol.kind.as_str(),
                symbol.name
            ),
            Err(e) => refused.push(format!(
                "{} {} ({}) → {e}",
                symbol.kind.as_str(),
                symbol.name,
                symbol.file.display()
            )),
        }
    }

    assert!(
        refused.is_empty(),
        "`fr unused` named these and `fr delete` would not remove them:\n  {}",
        refused.join("\n  ")
    );
    assert_eq!(
        checked,
        unused.len(),
        "every candidate has to be asked, and {checked} of {} were",
        unused.len()
    );
}

#[test]
fn a_class_declared_twice_and_used_once_is_not_dead() {
    // The use resolves to one of the declarations. Counting per site made the other
    // one look unreferenced, so the report named a class the markup was using.
    let files = polyglot();
    let (_tmp, index) = workspace(&files);
    let unused = delete::find_unused(&index, &Entrypoints::none());
    let dead: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();

    for live in ["panel", "note"] {
        assert!(
            !dead.contains(&live),
            "`.{live}` is used by the markup and declared in two stylesheets; \
             neither declaration is dead. Reported: {dead:?}"
        );
    }
    // The negative half: a class nothing uses must still be found, in both files.
    for gone in ["never-used", "also-never"] {
        assert!(
            dead.contains(&gone),
            "`.{gone}` is used by nothing and should be reported. Reported: {dead:?}"
        );
    }
}

#[test]
fn refs_on_one_declaration_finds_every_use_of_the_class() {
    // `fr refs` on the second declaration of a CSS class used to report nothing,
    // while `fr rename` at the same position changed five sites. Looking before you
    // leap has to see what the leap will do.
    let files = polyglot();
    let (_tmp, index) = workspace(&files);

    let declarations: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.name == "panel" && s.kind == fun_refactor::model::SymbolKind::Selector)
        .map(|s| (s.id, s.file.clone()))
        .collect();
    assert_eq!(
        declarations.len(),
        2,
        "the fixture declares `.panel` in two stylesheets; got {declarations:?}"
    );

    let counts: Vec<usize> = declarations
        .iter()
        .map(|(id, _)| index.references_to(*id).len())
        .collect();
    assert!(
        counts[0] > 0 && counts[0] == counts[1],
        "both declarations of one class must report the same uses, got {counts:?}"
    );
}

/// A package clause is not dead code, because it is not code that can die.
///
/// Java classes in one package never write the package's name and nothing can import
/// Go's `main`, so "nothing uses this" is true of every package declaration and says
/// nothing about any of them. `spring-petclinic` reported all forty-nine of its, one per
/// file. Removing one is a syntax error, not a refactoring.
#[test]
fn a_package_clause_is_not_reported_as_unused() {
    let (_tmp, index) = workspace(&[
        (
            "A.java",
            "package app;\n\npublic class A {\n    public static void main(String[] a) {\n        new B();\n    }\n}\n",
        ),
        ("B.java", "package app;\n\npublic class B {\n}\n"),
        ("main.go", "package main\n\nfunc main() {}\n"),
    ]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let modules: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.kind == fun_refactor::model::SymbolKind::Module)
        .map(|s| format!("{} in {}", s.name, s.file.display()))
        .collect();
    assert!(modules.is_empty(), "package clauses reported: {modules:?}");
}

/// The other side: Rust's `mod` wears the same symbol kind and means something else.
/// It declares a child module, and one nothing references is a real finding.
#[test]
fn an_unreferenced_rust_module_is_still_reported() {
    let (_tmp, index) = workspace(&[("a.rs", "mod helper;\n\nfn main() {}\n")]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let found: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.kind == fun_refactor::model::SymbolKind::Module)
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(found, vec!["helper".to_string()], "a `mod` can be dead");
}

/// A class whose methods are entry points is reached, whatever calls them.
///
/// JUnit constructs a test class to run the `@Test` methods in it, and the
/// class itself is named nowhere, `spring-petclinic` reported eleven of them. The rule
/// asks the containment chain instead of the language, so a Rust `mod tests` and a
/// Python class of pytest cases are covered by the same sentence.
#[test]
fn a_container_of_an_entry_point_is_not_dead() {
    let (_tmp, index) = workspace(&[(
        "OwnerTests.java",
        "package app;\n\nclass OwnerTests {\n    @Test\n    void findsAnOwner() {\n    }\n\n    \
         void helper() {\n    }\n}\n",
    )]);

    let test_method = only_symbol(&index, "findsAnOwner");
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[test_method]));
    let names: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect();

    assert!(
        !names.contains(&"OwnerTests".to_string()),
        "the class holding the test is reached: {names:?}"
    );
    assert!(
        names.contains(&"helper".to_string()),
        "a method beside the test is still judged on its own: {names:?}"
    );
}

/// A JavaBean accessor is reached by its property, never by its name.
///
/// `${owner.address}` in a template reaches `Owner::getAddress`. The property was in the
/// string index already; only the question was missing. Java only. There the convention
/// is a specification that template engines, JSON mappers and Spring's binder all follow.
#[test]
fn a_bean_accessor_named_only_by_its_property_is_spared() {
    let (_tmp, index) = workspace(&[
        (
            "Owner.java",
            "package app;\n\npublic class Owner {\n    private String address;\n\n    \
             public String getAddress() {\n        return this.address;\n    }\n\n    \
             public String getSecret() {\n        return \"\";\n    }\n}\n",
        ),
        (
            "list.html",
            "<html><body><td th:text=\"${owner.address}\"></td></body></html>\n",
        ),
    ]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let names: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect();

    assert!(
        !names.contains(&"getAddress".to_string()),
        "the template names its property: {names:?}"
    );
    assert!(
        names.contains(&"getSecret".to_string()),
        "an accessor nothing names either way is still a finding: {names:?}"
    );
}

/// `gettysburg` is not an accessor for `tysburg`.
#[test]
fn a_name_merely_starting_with_get_is_not_an_accessor() {
    let (_tmp, index) = workspace(&[
        (
            "A.java",
            "package app;\n\npublic class A {\n    public String gettysburg() {\n        \
             return \"\";\n    }\n}\n",
        ),
        ("notes.md", "# tysburg\n\nProse mentioning tysburg.\n"),
    ]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let names: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect();
    assert!(
        names.contains(&"gettysburg".to_string()),
        "the property rule needs an uppercase letter after the prefix: {names:?}"
    );
}

/// An attribute value is a string the HTML grammar happens not to call one.
///
/// It is also where a template names the code behind it, so the rule meant to catch
/// exactly that — "spared because its name is spelled in a string", could not see the
/// whole Thymeleaf, Vue and Angular way of referring to code.
#[test]
fn a_name_in_a_template_attribute_counts_as_named() {
    let (_tmp, index) = workspace(&[
        (
            "app.ts",
            "export function submitOrder() {\n  return 1;\n}\n\n\
             export function neverMentioned() {\n  return 2;\n}\n",
        ),
        (
            "page.html",
            "<html><body><button v-on:click=\"submitOrder\">Go</button></body></html>\n",
        ),
    ]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let names: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect();
    assert!(
        !names.contains(&"submitOrder".to_string()),
        "the template names it: {names:?}"
    );
    assert!(
        names.contains(&"neverMentioned".to_string()),
        "and one nothing names is still a finding: {names:?}"
    );
}

/// An HCL block Terraform gives no address to.
///
/// `terraform {}`, `required_providers {}`, `lifecycle {}` and a `dynamic` block's
/// `content {}` carry no label, so nothing can reference one and every single one
/// answers "nothing uses this". terraform-aws-vpc reported 46, all of that shape.
/// A labelled block takes its name from a string label, so the quote before the name
/// is the test, no list of block types to keep up with Terraform.
#[test]
fn an_unaddressable_hcl_block_is_not_reported() {
    let (_tmp, index) = workspace(&[(
        "main.tf",
        "terraform {\n  required_providers {\n    aws = {\n      source = \"hashicorp/aws\"\n    }\n  }\n}\n\n\
         resource \"aws_vpc\" \"this\" {\n  cidr_block = \"10.0.0.0/16\"\n\n  \
         lifecycle {\n    create_before_destroy = true\n  }\n}\n\n\
         output \"id\" {\n  value = aws_vpc.this.id\n}\n",
    )]);

    let unused = delete::find_unused(&index, &Entrypoints::none());
    let names: Vec<_> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.qualified_name())
        .collect();
    for unaddressable in ["terraform", "required_providers", "aws_vpc::lifecycle"] {
        assert!(
            !names.contains(&unaddressable.to_string()),
            "{unaddressable} has no address, so nothing can reference it: {names:?}"
        );
    }
}

/// A Zig test block, and what only it calls, are not dead.
#[test]
fn a_zig_test_block_and_what_it_calls_are_reached() {
    let (_tmp, index) = workspace(&[(
        "a.zig",
        "const std = @import(\"std\");\n\n\
         fn helper() i32 {\n    return 7;\n}\n\n\
         fn nothing_calls_this() i32 {\n    return 1;\n}\n\n\
         test \"helper returns seven\" {\n    \
         try std.testing.expectEqual(@as(i32, 7), helper());\n}\n",
    )]);
    let entrypoints = Entrypoints::detect(&index).expect("the built-in catalogs");

    let names: Vec<_> = delete::find_unused(&index, &entrypoints)
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect();
    assert!(
        !names.contains(&"helper returns seven".to_string()),
        "the test is an entry point: {names:?}"
    );
    assert!(
        !names.contains(&"helper".to_string()),
        "the test calls it: {names:?}"
    );
    assert!(
        names.contains(&"nothing_calls_this".to_string()),
        "and a function no test calls is still a finding: {names:?}"
    );
}
