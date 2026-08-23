//! Extract and inline for the cells the matrix still refused: Bash variables and
//! functions, Zig functions, SCSS mixins and XML entities.
//!
//! Every test asserts the exact resulting file text instead of a substring, checks
//! that the bytes outside the edited ranges came through unchanged, and puts the plan
//! through `edit::plan(.., ReparseStrict)` so a refactoring that would break the file
//! fails here and not on disk.

use fun_refactor::edit::{apply_to_string, plan, Edit, Validation};
use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::refactor::{extract, inline, Refusal};
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::Span;
use std::path::{Path, PathBuf};

struct Workspace {
    tmp: tempfile::TempDir,
    index: Index,
}

impl Workspace {
    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    /// Re-read the tree from disk. Used after applying one refactoring so the next one
    /// resolves against the rewritten file, which a round trip needs.
    fn reindex(&mut self) {
        let scanned = scan(self.tmp.path(), &ScanOptions::default()).unwrap();
        self.index = Index::build_from_scan(&scanned).unwrap();
    }
}

fn workspace(files: &[(&str, &str)]) -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    Workspace { tmp, index }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn applied(edits: &fun_refactor::edit::EditSet, path: &Path) -> String {
    let before = read(path);
    let after = apply_to_string(&before, edits.edits_for(path).unwrap()).unwrap();
    untouched_regions_survive(&before, &after, edits.edits_for(path).unwrap());
    after
}

/// Every byte outside an edited range must appear unchanged, in order, in the result.
fn untouched_regions_survive(before: &str, after: &str, edits: &[Edit]) {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|e| (e.span.start, e.span.end));

    let mut old = 0usize;
    let mut new = 0usize;
    for edit in ordered {
        let gap = edit.span.start - old;
        assert_eq!(
            &before[old..old + gap],
            &after[new..new + gap],
            "bytes outside the edited ranges changed"
        );
        old = edit.span.end;
        new += gap + edit.replacement.len();
    }
    assert_eq!(
        &before[old..],
        &after[new..],
        "bytes after the last edit changed"
    );
}

fn must_reparse(edits: &fun_refactor::edit::EditSet) {
    plan(edits, Validation::ReparseStrict).expect("the result must still parse");
}

/// A 1-based inclusive line range, as a byte span.
fn lines(source: &str, first: usize, last: usize) -> Span {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let start = starts[first - 1];
    let end = starts
        .get(last)
        .copied()
        .unwrap_or(source.len())
        .min(source.len());
    Span::new(start, end)
}

fn at(source: &str, needle: &str) -> Span {
    let start = source.find(needle).unwrap_or_else(|| panic!("{needle:?}"));
    Span::new(start, start + needle.len())
}

fn symbol_at(index: &Index, path: &Path, offset: usize) -> SymbolId {
    index
        .definition_at(path, offset)
        .expect("a definition at that offset")
        .id
}

#[test]
fn bash_extract_names_a_command_substitution() {
    let src = "#!/bin/bash\nresult=$(compute a)\necho \"$result\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out =
        extract::variable(&ws.index, &path, at(src, "$(compute a)"), "stamp", false).unwrap();

    assert_eq!(plan_out.expression, "$(compute a)");
    assert_eq!(plan_out.occurrences, 1);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\nstamp=$(compute a)\nresult=\"$stamp\"\necho \"$result\"\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_leaves_a_splitting_expansion_unquoted() {
    // `printf %s $(list)` splits the output on $IFS. `"$items"` would make it one
    // word, which is a different command line, so the reference stays bare.
    let src = "#!/bin/bash\nprintf %s $(list)\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out = extract::variable(&ws.index, &path, at(src, "$(list)"), "items", false).unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\nitems=$(list)\nprintf %s $items\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_uses_braces_inside_double_quotes() {
    // A second pair of quotes would end the string, and inside one the expansion is
    // already protected from splitting.
    let src = "#!/bin/bash\necho \"at $(date)\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out = extract::variable(&ws.index, &path, at(src, "$(date)"), "now", false).unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\nnow=$(date)\necho \"at ${now}\"\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_quotes_a_literal_string() {
    let src = "#!/bin/bash\necho \"hello world\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out = extract::variable(
        &ws.index,
        &path,
        at(src, "\"hello world\""),
        "greeting",
        false,
    )
    .unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\ngreeting=\"hello world\"\necho \"$greeting\"\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_replaces_every_occurrence_when_asked() {
    let src = "#!/bin/bash\na=$(id -u)\nb=$(id -u)\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out = extract::variable(&ws.index, &path, at(src, "$(id -u)"), "uid", true).unwrap();

    assert_eq!(plan_out.occurrences, 2);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\nuid=$(id -u)\na=\"$uid\"\nb=\"$uid\"\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_never_rewrites_an_occurrence_before_the_binding() {
    // The first occurrence is above the insertion point, where the variable is not
    // set yet, so it is left as it was.
    let src = "#!/bin/bash\na=$(id -u)\nb=$(id -u)\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let second = src.rfind("$(id -u)").unwrap();
    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(second, second + "$(id -u)".len()),
        "uid",
        true,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 1);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\na=$(id -u)\nuid=$(id -u)\nb=\"$uid\"\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_refuses_an_existing_expansion() {
    let src = "#!/bin/bash\nX=1\necho \"$X\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::variable(&ws.index, &path, at(src, "$X"), "alias", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("already a variable expansion"), "got: {err}");
}

#[test]
fn bash_extract_refuses_the_condition_of_an_if() {
    let src = "#!/bin/bash\nif [ -n \"$(peek)\" ]; then\n  echo yes\nfi\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::variable(&ws.index, &path, at(src, "$(peek)"), "seen", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("condition of an `if`"), "got: {err}");
}

#[test]
fn bash_extract_refuses_a_loop_condition() {
    let src = "#!/bin/bash\nwhile [ -n \"$(peek)\" ]; do\n  step\ndone\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::variable(&ws.index, &path, at(src, "$(peek)"), "seen", false)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("re-evaluates on every iteration"),
        "got: {err}"
    );
}

#[test]
fn bash_extract_refuses_a_command_name() {
    let src = "#!/bin/bash\ndate +%s\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::variable(&ws.index, &path, at(src, "date"), "tool", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("name of a command"), "got: {err}");
}

#[test]
fn bash_extract_refuses_an_invalid_name() {
    let src = "#!/bin/bash\necho \"$(date)\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::variable(&ws.index, &path, at(src, "$(date)"), "2now", false).unwrap_err();
    assert!(
        err.downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::InvalidName { .. })),
        "got: {err}"
    );
}

#[test]
fn bash_extract_refuses_a_name_already_in_use() {
    let src = "#!/bin/bash\nnow=1\necho \"$(date)\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::variable(&ws.index, &path, at(src, "$(date)"), "now", false).unwrap_err();
    assert!(
        err.downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::NameCollision { .. })),
        "got: {err}"
    );
}

#[test]
fn bash_inline_takes_a_quoted_values_contents_inside_quotes() {
    let src = "#!/bin/bash\ngreeting=\"hello world\"\necho \"$greeting\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("greeting=").unwrap());

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 1);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\necho \"hello world\"\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_inline_substitutes_a_command_substitution_verbatim() {
    let src = "#!/bin/bash\nitems=$(list)\nprintf %s $items\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("items=").unwrap());

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\nprintf %s $(list)\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_inline_removes_a_whole_local_declaration() {
    // `local now=…` owns its line; taking only the assignment would leave `local`.
    let src = "#!/bin/bash\nf() {\n  local now=$(date)\n  echo \"at ${now}\"\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("now=").unwrap());

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\nf() {\n  echo \"at $(date)\"\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_inline_refuses_a_second_assignment() {
    let src = "#!/bin/bash\nx=1\necho \"$x\"\nx=2\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("x=1").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("assigned again at line 4"), "got: {err}");
    assert!(err.contains("no block scope"), "got: {err}");
}

#[test]
fn bash_inline_refuses_an_exported_variable() {
    let src = "#!/bin/bash\nexport TOKEN=abc\necho \"$TOKEN\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("TOKEN=").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("exported"), "got: {err}");
    assert!(err.contains("child process"), "got: {err}");
}

#[test]
fn bash_inline_refuses_when_the_name_appears_inside_single_quotes() {
    // The shell expands nothing between single quotes, so that text is not a use,
    // and deleting the assignment would leave it reading like one.
    let src = "#!/bin/bash\nmsg=hello\necho \"$msg\"\necho 'literal $msg'\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("msg=").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("inside single quotes"), "got: {err}");
    assert!(err.contains("expands nothing"), "got: {err}");
}

#[test]
fn bash_inline_refuses_a_parameter_expansion_operator() {
    let src = "#!/bin/bash\nname=world\necho \"${name:-none}\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("name=").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("parameter expansion operator"), "got: {err}");
}

#[test]
fn bash_inline_refuses_a_command_prefix_assignment() {
    let src = "#!/bin/bash\nFOO=bar cmd\necho \"$FOO\"\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("FOO=").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("prefix of a single command"), "got: {err}");
}

#[test]
fn bash_inline_refuses_an_unquoted_use_of_a_multi_word_value() {
    let src = "#!/bin/bash\nfiles=\"a b\"\nls $files\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("files=").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("splits on `$IFS`"), "got: {err}");
}

#[test]
fn bash_inline_refuses_a_loop_variable() {
    let src = "#!/bin/bash\nfor item in a b; do\n  echo \"$item\"\ndone\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("item in").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("not bound by an assignment"), "got: {err}");
}

#[test]
fn bash_inline_refuses_a_binding_with_no_uses() {
    let src = "#!/bin/bash\nunused=1\necho hi\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");
    let id = symbol_at(&ws.index, &path, src.find("unused=").unwrap());

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("no uses"), "got: {err}");
}

#[test]
fn bash_extract_then_inline_restores_the_original_bytes() {
    // Three shapes, one per quoting rule: an assignment right-hand side, an unquoted
    // splitting position and a position inside double quotes.
    for src in [
        "#!/bin/bash\nresult=$(compute a)\necho \"$result\"\n",
        "#!/bin/bash\nprintf %s $(list)\n",
        "#!/bin/bash\necho \"at $(date)\"\n",
    ] {
        let mut ws = workspace(&[("run.sh", src)]);
        let path = ws.path("run.sh");
        let selection = at(
            src,
            if src.contains("compute") {
                "$(compute a)"
            } else if src.contains("list") {
                "$(list)"
            } else {
                "$(date)"
            },
        );

        let extracted = extract::variable(&ws.index, &path, selection, "tmp", false).unwrap();
        let after = applied(&extracted.edits, &path);
        std::fs::write(&path, &after).unwrap();
        ws.reindex();

        let id = symbol_at(&ws.index, &path, after.find("tmp=").unwrap());
        let inlined = inline::variable(&ws.index, id).unwrap();
        assert_eq!(applied(&inlined.edits, &path), src, "round trip of:\n{src}");
    }
}

#[test]
fn bash_extract_function_defines_it_before_the_enclosing_one() {
    let src = "#!/bin/bash\n\ndeploy() {\n  echo start\n  build\n  test\n  echo done\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out = extract::function(&ws.index, &path, lines(src, 5, 6), "prepare").unwrap();

    assert!(plan_out.parameters.is_empty(), "shell has no block scope");
    assert!(plan_out.returns.is_empty());
    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\n\nprepare() {\n  build\n  test\n}\n\ndeploy() {\n  echo start\n  prepare\n  echo done\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_function_at_top_level_goes_below_the_shebang() {
    let src = "#!/bin/bash\n# setup\necho one\necho two\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let plan_out = extract::function(&ws.index, &path, lines(src, 3, 3), "first").unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "#!/bin/bash\n# setup\nfirst() {\n  echo one\n}\n\nfirst\necho two\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn bash_extract_function_refuses_positional_parameters() {
    let src = "#!/bin/bash\ngreet() {\n  echo \"hi $1\"\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::function(&ws.index, &path, lines(src, 3, 3), "say")
        .unwrap_err()
        .to_string();
    assert!(err.contains("$1"), "got: {err}");
    assert!(err.contains("rebind"), "got: {err}");
}

#[test]
fn bash_extract_function_refuses_an_escaping_return() {
    let src = "#!/bin/bash\nf() {\n  check || return 1\n  echo ok\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::function(&ws.index, &path, lines(src, 3, 3), "guard")
        .unwrap_err()
        .to_string();
    assert!(err.contains("`return`"), "got: {err}");
}

#[test]
fn bash_extract_function_refuses_half_a_statement() {
    let src = "main() {\n  echo hi\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::function(&ws.index, &path, Span::new(0, 5), "f")
        .unwrap_err()
        .to_string();
    assert!(err.contains("cuts across"), "got: {err}");
}

#[test]
fn bash_extract_function_refuses_moving_a_local_that_is_read_later() {
    let src = "#!/bin/bash\nf() {\n  local n=1\n  echo \"$n\"\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::function(&ws.index, &path, lines(src, 3, 3), "setup")
        .unwrap_err()
        .to_string();
    assert!(err.contains("local n"), "got: {err}");
}

#[test]
fn bash_extract_function_refuses_a_name_already_in_use() {
    let src = "#!/bin/bash\nf() {\n  echo hi\n}\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::function(&ws.index, &path, lines(src, 3, 3), "f").unwrap_err();
    assert!(
        err.downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::NameCollision { .. })),
        "got: {err}"
    );
}

#[test]
fn bash_extract_function_refuses_a_blank_selection() {
    let src = "#!/bin/bash\n\necho hi\n";
    let ws = workspace(&[("run.sh", src)]);
    let path = ws.path("run.sh");

    let err = extract::function(&ws.index, &path, lines(src, 2, 2), "nothing")
        .unwrap_err()
        .to_string();
    assert!(err.contains("blank"), "got: {err}");
}

#[test]
fn zig_extract_function_writes_the_return_type_without_an_arrow() {
    let src = "fn run() void {\n    const width: i32 = 3;\n    const area: i32 = width * 2;\n    log(area);\n}\n";
    let ws = workspace(&[("a.zig", src)]);
    let path = ws.path("a.zig");

    let plan_out = extract::function(&ws.index, &path, lines(src, 3, 3), "compute").unwrap();

    assert_eq!(plan_out.parameters.len(), 1);
    assert_eq!(plan_out.parameters[0].name, "width");
    assert_eq!(
        plan_out.parameters[0].type_annotation.as_deref(),
        Some("i32")
    );
    assert_eq!(plan_out.returns, vec!["area".to_string()]);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "fn run() void {\n    const width: i32 = 3;\n    const area = compute(width);\n    log(area);\n}\n\nfn compute(width: i32) i32 {\n    const area: i32 = width * 2;\n    return area;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn zig_extract_function_says_void_when_nothing_comes_back() {
    let src = "fn run() void {\n    const width: i32 = 3;\n    log(width);\n}\n";
    let ws = workspace(&[("a.zig", src)]);
    let path = ws.path("a.zig");

    let plan_out = extract::function(&ws.index, &path, lines(src, 3, 3), "show").unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "fn run() void {\n    const width: i32 = 3;\n    show(width);\n}\n\nfn show(width: i32) void {\n    log(width);\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn zig_extract_function_refuses_the_selection_whose_type_was_never_written() {
    // Zig is refused per selection, like Rust and Go: the language needing a
    // written type is not the reason, the missing annotation is.
    let src = "fn run() void {\n    const width = 3;\n    log(width);\n}\n";
    let ws = workspace(&[("a.zig", src)]);
    let path = ws.path("a.zig");

    let err = extract::function(&ws.index, &path, lines(src, 3, 3), "show")
        .unwrap_err()
        .to_string();
    assert!(err.contains("never written down"), "got: {err}");
    assert!(err.contains("width"), "got: {err}");
    assert!(err.contains("zig"), "got: {err}");
}

#[test]
fn scss_extract_mixin_passes_outside_variables_as_parameters() {
    let src = "$pad: 4px;\n\n.btn {\n  color: red;\n  padding: $pad;\n  margin: 0;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");

    let plan_out = extract::function(&ws.index, &path, lines(src, 4, 5), "base").unwrap();

    assert_eq!(plan_out.parameters.len(), 1);
    assert_eq!(plan_out.parameters[0].name, "$pad");
    assert_eq!(
        applied(&plan_out.edits, &path),
        "@mixin base($pad) {\n  color: red;\n  padding: $pad;\n}\n\n$pad: 4px;\n\n.btn {\n  @include base($pad);\n  margin: 0;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn scss_extract_mixin_without_parameters_takes_none() {
    let src = ".btn {\n  color: red;\n  margin: 0;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");

    let plan_out = extract::function(&ws.index, &path, lines(src, 2, 2), "plain").unwrap();

    assert!(plan_out.parameters.is_empty());
    assert_eq!(
        applied(&plan_out.edits, &path),
        "@mixin plain {\n  color: red;\n}\n\n.btn {\n  @include plain;\n  margin: 0;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn scss_extract_mixin_keeps_a_variable_the_selection_declares_itself() {
    let src = ".btn {\n  $local: 2px;\n  padding: $local;\n  margin: 0;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");

    let plan_out = extract::function(&ws.index, &path, lines(src, 2, 3), "pad").unwrap();

    assert!(
        plan_out.parameters.is_empty(),
        "a variable declared inside the selection travels with it: {:?}",
        plan_out.parameters
    );
    assert_eq!(
        applied(&plan_out.edits, &path),
        "@mixin pad {\n  $local: 2px;\n  padding: $local;\n}\n\n.btn {\n  @include pad;\n  margin: 0;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn css_extract_function_stays_refused_because_css_has_no_mixin() {
    let src = ".btn {\n  color: red;\n}\n";
    let ws = workspace(&[("theme.css", src)]);
    let path = ws.path("theme.css");

    let err = extract::function(&ws.index, &path, lines(src, 2, 2), "base")
        .unwrap_err()
        .to_string();
    assert!(err.contains("no mixin"), "got: {err}");
    assert!(err.contains("Sass"), "got: {err}");
}

#[test]
fn scss_extract_mixin_refuses_a_nested_rule() {
    let src = ".btn {\n  color: red;\n  &:hover { color: blue; }\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");

    let err = extract::function(&ws.index, &path, lines(src, 2, 3), "base")
        .unwrap_err()
        .to_string();
    assert!(err.contains("only declarations"), "got: {err}");
}

#[test]
fn scss_extract_mixin_refuses_a_selection_outside_a_rule() {
    let src = "$pad: 4px;\n\n.btn {\n  color: red;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");

    let err = extract::function(&ws.index, &path, lines(src, 1, 1), "base")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not inside a rule"), "got: {err}");
}

#[test]
fn scss_extract_mixin_refuses_a_name_already_in_use() {
    let src = "@mixin base { color: red; }\n\n.btn {\n  margin: 0;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");

    let err = extract::function(&ws.index, &path, lines(src, 4, 4), "base").unwrap_err();
    assert!(
        err.downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::NameCollision { .. })),
        "got: {err}"
    );
}

#[test]
fn xml_extract_creates_the_internal_subset_when_there_is_none() {
    let src = "<root title=\"Acme\">\n  <a>Acme here</a>\n</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let plan_out = extract::variable(&ws.index, &path, at(src, "Acme"), "brand", true).unwrap();

    assert_eq!(plan_out.occurrences, 2);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "<!DOCTYPE root [\n  <!ENTITY brand \"Acme\">\n]>\n<root title=\"&brand;\">\n  <a>&brand; here</a>\n</root>\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn xml_extract_joins_an_internal_subset_that_already_exists() {
    let src = "<!DOCTYPE root [\n  <!ENTITY other \"x\">\n]>\n<root title=\"Acme\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let plan_out = extract::variable(&ws.index, &path, at(src, "Acme"), "brand", false).unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "<!DOCTYPE root [\n  <!ENTITY other \"x\">\n  <!ENTITY brand \"Acme\">\n]>\n<root title=\"&brand;\">y</root>\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn xml_extract_opens_a_subset_on_a_bare_doctype() {
    let src = "<!DOCTYPE root>\n<root title=\"Acme\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let plan_out = extract::variable(&ws.index, &path, at(src, "Acme"), "brand", false).unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "<!DOCTYPE root [\n  <!ENTITY brand \"Acme\">\n]>\n<root title=\"&brand;\">y</root>\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn xml_extract_refuses_a_predefined_entity_name() {
    let src = "<root title=\"Acme\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = extract::variable(&ws.index, &path, at(src, "Acme"), "amp", false).unwrap_err();
    assert!(
        err.downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::InvalidName { .. })),
        "got: {err}"
    );
}

#[test]
fn xml_extract_refuses_markup_inside_the_value() {
    let src = "<root><a>50% off</a></root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = extract::variable(&ws.index, &path, at(src, "50% off"), "deal", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("markup inside an entity value"), "got: {err}");
}

#[test]
fn xml_extract_refuses_a_selection_that_is_not_text() {
    let src = "<root><a>hello</a></root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = extract::variable(&ws.index, &path, Span::new(1, 5), "x", false)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("no attribute value or element text"),
        "got: {err}"
    );
}

#[test]
fn xml_extract_refuses_an_entity_that_is_already_declared() {
    let src = "<!DOCTYPE root [\n  <!ENTITY brand \"x\">\n]>\n<root title=\"Acme\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = extract::variable(&ws.index, &path, at(src, "Acme"), "brand", false).unwrap_err();
    assert!(
        err.downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::NameCollision { .. })),
        "got: {err}"
    );
}

#[test]
fn xml_inline_is_the_exact_inverse_of_the_extraction() {
    let src = "<root title=\"Acme\">\n  <a>Acme here</a>\n</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let extracted = extract::variable(&ws.index, &path, at(src, "Acme"), "brand", true).unwrap();
    let after = applied(&extracted.edits, &path);
    std::fs::write(&path, &after).unwrap();

    let inlined = inline::xml_entity(&path, "brand").unwrap();
    assert_eq!(inlined.use_sites, 2);
    assert_eq!(inlined.value, "Acme");
    assert_eq!(applied(&inlined.edits, &path), src);
    must_reparse(&inlined.edits);
}

#[test]
fn xml_inline_keeps_a_subset_that_declares_something_else() {
    let src = "<!DOCTYPE root [\n  <!ENTITY other \"x\">\n  <!ENTITY brand \"Acme\">\n]>\n<root title=\"&brand;\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let inlined = inline::xml_entity(&path, "brand").unwrap();
    assert_eq!(
        applied(&inlined.edits, &path),
        "<!DOCTYPE root [\n  <!ENTITY other \"x\">\n]>\n<root title=\"Acme\">y</root>\n"
    );
    must_reparse(&inlined.edits);
}

#[test]
fn xml_inline_refuses_a_document_with_no_internal_subset() {
    let src = "<root title=\"Acme\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = inline::xml_entity(&path, "brand").unwrap_err().to_string();
    assert!(err.contains("no `<!DOCTYPE"), "got: {err}");
}

#[test]
fn xml_inline_refuses_an_entity_that_is_never_referenced() {
    let src = "<!DOCTYPE root [\n  <!ENTITY brand \"Acme\">\n]>\n<root>y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = inline::xml_entity(&path, "brand").unwrap_err().to_string();
    assert!(err.contains("never referenced"), "got: {err}");
}

#[test]
fn xml_inline_refuses_a_value_that_would_end_the_attribute() {
    let src =
        "<!DOCTYPE root [\n  <!ENTITY brand 'say \"hi\"'>\n]>\n<root title=\"&brand;\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let err = inline::xml_entity(&path, "brand").unwrap_err().to_string();
    assert!(err.contains("delimits the attribute value"), "got: {err}");
}

#[test]
fn xml_entities_are_symbols_so_inline_is_reachable_by_name() {
    // `queries/xml/facts.scm` declares entities, so an entity has a SymbolId and
    // `fr inline` routes to it like any other binding.
    let src =
        "<!DOCTYPE root [\n  <!ENTITY brand \"Acme\">\n]>\n<root title=\"&brand;\">y</root>\n";
    let ws = workspace(&[("doc.xml", src)]);
    let path = ws.path("doc.xml");

    let entity = ws
        .index
        .find_symbols("brand", None)
        .first()
        .copied()
        .expect("the entity is a symbol")
        .id;

    // And both use sites resolve to it, so an inline reaches every one.
    let uses = ws.index.references_to(entity);
    assert_eq!(uses.len(), 1, "got {uses:?}");

    let plan_out = inline::variable(&ws.index, entity).expect("inline routes to the entity");
    let updated = applied(&plan_out.edits, &path);
    assert!(!updated.contains("&brand;"), "got:\n{updated}");
    assert!(updated.contains("Acme"), "got:\n{updated}");
}
