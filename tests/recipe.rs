//! The recipe language: what it accepts, what it refuses, and what running one does.

use fun_refactor::recipe::{self, Operation, Options};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn workspace(
    files: &[(&str, &str)],
) -> (
    tempfile::TempDir,
    BTreeMap<PathBuf, (fun_refactor::lang::Language, String)>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let mut sources = BTreeMap::new();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        let language = fun_refactor::lang::detect(&path).expect("a language");
        sources.insert(path, (language, content.to_string()));
    }
    (tmp, sources)
}

const AUTH: &str = r#"USE_LEGACY_AUTH = False


def authenticate(user, token):
    if USE_LEGACY_AUTH:
        return legacy_auth_check(user, token)
    return modern_auth_check(user, token)


def legacy_auth_check(user, token):
    return token == user.legacy_token


def legacy_auth_header(request):
    return request.headers.get("X-Legacy-Auth")


def modern_auth_check(user, token):
    return user.verify(token)
"#;

#[test]
fn the_schema_comes_first_and_is_mandatory() {
    // It makes a staged answer to sharing possible: a reader can refuse a file
    // it does not understand before it has parsed a single step.
    let error = recipe::parse("recipe r { delete where unused }").unwrap_err();
    assert!(error.to_string().contains("expects `schema`"), "{error}");

    let error = recipe::parse("schema 99\nrecipe r { delete where unused }").unwrap_err();
    assert!(
        error.to_string().contains("understands schema 1"),
        "a version this build was not written for must be refused, not guessed at: {error}"
    );
}

#[test]
fn a_where_clause_runs_across_as_many_lines_as_it_needs() {
    // Layout is insignificant and statements are not terminated; a statement ends when
    // a token appears that can only begin a new one.
    let file = recipe::parse(
        "schema 1\n\
         recipe r {\n\
           delete where kind=function\n\
                        name~\"legacy_*\"\n\
                        !exported\n\
                 on-refusal report\n\
         }\n",
    )
    .expect("this parses");
    let step = &file.recipes[0].steps[0];
    assert_eq!(step.selector.len(), 3);
    assert_eq!(step.on_refusal, recipe::OnRefusal::Report);
}

#[test]
fn where_and_the_modifiers_are_order_independent() {
    // Rejecting `delete on-refusal allow where unused` is a rule nobody remembers and
    // it buys nothing.
    let file = recipe::parse("schema 1\nrecipe r { delete on-refusal allow where unused }")
        .expect("this parses");
    assert_eq!(
        file.recipes[0].steps[0].on_refusal,
        recipe::OnRefusal::Allow
    );
}

#[test]
fn a_raw_string_needs_no_escaping() {
    // Patterns *are* code, and code is full of quotes.
    let file =
        recipe::parse("schema 1\nrecipe r { restructure python '\"%s\" % ($X,)' => 'f\"{$X}\"' }")
            .expect("this parses");
    let Operation::Restructure { pattern, .. } = &file.recipes[0].steps[0].operation else {
        panic!("expected a restructure");
    };
    assert_eq!(pattern, "\"%s\" % ($X,)");
}

#[test]
fn an_operation_that_needs_an_argument_says_so() {
    // All three of these parsed happily in the prototype and meant nothing.
    for (source, wanted) in [
        (
            "schema 1\nrecipe r { rewrite where lang=go }",
            "`rewrite` needs the transformation",
        ),
        (
            "schema 1\nrecipe r { rename where name=\"a\" }",
            "expects `to`",
        ),
        (
            "schema 1\nrecipe r { remove-flag \"F\" = false where unused }",
            "takes no `where` clause",
        ),
    ] {
        let error = recipe::parse(source).unwrap_err().to_string();
        assert!(error.contains(wanted), "expected {wanted:?}, got: {error}");
    }
}

#[test]
fn a_step_with_no_selector_is_refused() {
    // A step with no selector would act on everything, which is never what anyone
    // means, and silently acting on everything is the worst failure available.
    let error = recipe::parse("schema 1\nrecipe r { delete }")
        .unwrap_err()
        .to_string();
    assert!(error.contains("needs a `where` clause"), "{error}");
}

#[test]
fn a_value_may_not_be_a_bare_step_keyword() {
    // `where name=` followed by a newline and `imports` swallowed `imports` as the
    // value and then failed confusingly two tokens later.
    let error = recipe::parse("schema 1\nrecipe r {\n delete where name=\n imports\n}")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("found the step keyword 'imports'"),
        "{error}"
    );
}

#[test]
fn a_mistyped_predicate_suggests_the_one_that_was_meant() {
    let (tmp, sources) = workspace(&[("a.py", AUTH)]);
    let file = recipe::parse("schema 1\nrecipe r { delete where exportd }").unwrap();
    let error = recipe::run(
        &file.recipes[0],
        sources,
        &Options {
            root: tmp.path(),
            catalogs: &[],
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Did you mean `exported`"), "{error}");
}

#[test]
fn reserved_words_and_predicates_do_not_overlap() {
    // The invariant the whole layout-free grammar rests on.
    for predicate in recipe::PREDICATES {
        assert!(
            !recipe::RESERVED.contains(predicate),
            "`{predicate}` is both a predicate and a reserved word, so a `where` clause \
             containing it cannot be told from the start of the next statement"
        );
    }
}

fn run(
    files: &[(&str, &str)],
    source: &str,
) -> (
    tempfile::TempDir,
    recipe::Report,
    BTreeMap<PathBuf, (fun_refactor::lang::Language, String)>,
) {
    let (tmp, sources) = workspace(files);
    let file = recipe::parse(source).expect("the recipe parses");
    let (report, after) = recipe::run(
        &file.recipes[0],
        sources,
        &Options {
            root: tmp.path(),
            catalogs: &[],
        },
    )
    .expect("the recipe runs");
    fun_refactor::vfs::use_filesystem();
    (tmp, report, after)
}

#[test]
fn each_step_sees_what_the_previous_one_left() {
    // The whole reason a recipe is more than a shell loop.
    let (tmp, report, after) = run(
        &[("src/auth.py", AUTH)],
        "schema 1\n\
         recipe retire {\n\
           requires symbol \"USE_LEGACY_AUTH\"\n\
           remove-flag \"USE_LEGACY_AUTH\" = false\n\
           delete where kind=function name~\"legacy_auth_*\" unused\n\
           expect changed > 0 files\n\
           expect refusals = 0\n\
         }\n",
    );

    assert_eq!(report.steps.len(), 2);
    assert_eq!(report.steps[1].matched, 2, "both dead functions are found");
    assert_eq!(report.steps[1].applied, 2);
    assert!(report.ok, "{report:?}");

    let text = &after[&tmp.path().join("src/auth.py")].1;
    assert!(!text.contains("USE_LEGACY_AUTH"), "{text}");
    assert!(!text.contains("legacy_auth_check"), "{text}");
    assert!(!text.contains("legacy_auth_header"), "{text}");
    assert!(text.contains("modern_auth_check"), "{text}");
}

#[test]
fn two_symbols_in_one_file_do_not_produce_conflicting_edits() {
    // Each subject is planned against a freshly built index over the text the previous one
    // left.
    let (tmp, report, after) = run(
        &[(
            "a.py",
            "def alpha_one():\n    return 1\n\n\ndef alpha_two():\n    return 2\n\n\ndef keep():\n    return 3\n",
        )],
        "schema 1\nrecipe r { delete where kind=function name~\"alpha_*\" unused }",
    );
    assert_eq!(report.steps[0].applied, 2);
    let text = &after[&tmp.path().join("a.py")].1;
    assert!(
        !text.contains("alpha_one") && !text.contains("alpha_two"),
        "{text}"
    );
    assert!(text.contains("keep"), "{text}");
}

#[test]
fn a_file_the_rewrite_does_not_apply_to_is_not_a_refusal() {
    // `rewrite`'s selector chooses *files*, and a file with no wrapping `if` in it had nothing
    // to do.
    let (_tmp, report, _after) = run(
        &[
            (
                "pkg/a.go",
                "package pkg\n\nfunc A(x bool) {\n\tif x {\n\t\twork()\n\t}\n}\n",
            ),
            ("pkg/b.go", "package pkg\n\nfunc B() int {\n\treturn 1\n}\n"),
        ],
        "schema 1\nrecipe r { rewrite guard-clause where lang=go in=\"pkg/\" }",
    );
    assert_eq!(report.steps[0].matched, 2, "both files are selected");
    assert!(
        report.steps[0].refusals.is_empty(),
        "a file with nothing to do is not a refusal: {:?}",
        report.steps[0].refusals
    );
    assert_eq!(
        report.steps[0].applied, 1,
        "one site, in the one file that had one"
    );
    assert!(report.ok);
}

const CALLS_PY: &str = r#"def log(message):
    print(message)


def save(record):
    log("saving")
    return record


def load(key):
    log("loading")
    return key


def unrelated():
    return 1
"#;

#[test]
fn the_call_graph_answers_calls_and_called_by() {
    // Both directions come from one graph, and the graph is only built when a
    // predicate asks for it.
    let (_tmp, report, _after) = run(
        &[("src/app.py", CALLS_PY)],
        "schema 1\nrecipe r { rename to \"note\" where kind=function calls=\"log\" limit 1 }",
    );
    assert_eq!(
        report.steps[0].matched, 2,
        "`calls=\"log\"` selects the two functions that call it"
    );

    let (_tmp, report, _after) = run(
        &[("src/app.py", CALLS_PY)],
        "schema 1\nrecipe r { rename to \"note\" where kind=function called-by=\"save\" }",
    );
    assert_eq!(report.steps[0].matched, 1, "`save` calls only `log`");
}

#[test]
fn a_structural_shape_selects_the_symbols_containing_it() {
    let (_tmp, report, _after) = run(
        &[("src/app.py", CALLS_PY)],
        "schema 1\nrecipe r { delete where kind=function matches='log($X)' lang=python }",
    );
    assert_eq!(report.steps[0].matched, 2);
    assert_eq!(report.steps[0].applied, 2);
}

#[test]
fn a_shape_without_a_language_is_refused() {
    // The same text parses into a different tree in every language, so there is no
    // language-free answer to where a shape occurs.
    let (tmp, sources) = workspace(&[("src/app.py", CALLS_PY)]);
    let file = recipe::parse("schema 1\nrecipe r { delete where matches='log($X)' }").unwrap();
    let error = recipe::run(
        &file.recipes[0],
        sources,
        &Options {
            root: tmp.path(),
            catalogs: &[],
        },
    )
    .unwrap_err()
    .to_string();
    fun_refactor::vfs::use_filesystem();
    assert!(error.contains("needs `lang=` beside it"), "{error}");
}

#[test]
fn implements_selects_the_concrete_answers_to_an_abstraction() {
    let (_tmp, report, _after) = run(
        &[(
            "sink.ts",
            "export interface Sink {\n  write(line: string): void;\n}\n\n\
             export class FileSink implements Sink {\n  write(line: string): void {}\n}\n\n\
             export class NullSink implements Sink {\n  write(line: string): void {}\n}\n\n\
             export class Unrelated {\n  other(): void {}\n}\n",
        )],
        "schema 1\nrecipe r { rename to \"Renamed\" where implements=\"Sink\" kind=class limit 1 }",
    );
    assert_eq!(
        report.steps[0].matched, 2,
        "both classes implement it and `Unrelated` does not"
    );
}

#[test]
fn a_selector_that_matches_nothing_stops_the_recipe() {
    // Silently doing nothing is the failure this most wants to avoid, because it looks
    // like success.
    let (tmp, sources) = workspace(&[("a.py", AUTH)]);
    let file =
        recipe::parse("schema 1\nrecipe r { delete where name=\"nothing_called_this\" }").unwrap();
    let error = recipe::run(
        &file.recipes[0],
        sources,
        &Options {
            root: tmp.path(),
            catalogs: &[],
        },
    )
    .unwrap_err()
    .to_string();
    fun_refactor::vfs::use_filesystem();
    assert!(error.contains("matched nothing"), "{error}");
    assert!(
        error.contains("allow-empty"),
        "the way out has to be named: {error}"
    );
}

#[test]
fn allow_empty_is_the_way_to_say_a_step_is_conditional() {
    let (_tmp, report, _after) = run(
        &[("a.py", AUTH)],
        "schema 1\n\
         recipe r {\n\
           delete where name=\"nothing_called_this\" allow-empty\n\
           rename to \"verify\" where name=\"modern_auth_check\"\n\
         }\n",
    );
    assert_eq!(report.steps[0].matched, 0);
    assert_eq!(report.steps[1].applied, 1);
}

#[test]
fn a_requirement_that_does_not_hold_refuses_before_anything_runs() {
    let (tmp, sources) = workspace(&[("a.py", AUTH)]);
    let file =
        recipe::parse("schema 1\nrecipe r { requires symbol \"NOT_HERE\"\n delete where unused }")
            .unwrap();
    let error = recipe::run(
        &file.recipes[0],
        sources,
        &Options {
            root: tmp.path(),
            catalogs: &[],
        },
    )
    .unwrap_err()
    .to_string();
    fun_refactor::vfs::use_filesystem();
    assert!(error.contains("written for a different tree"), "{error}");
}

#[test]
fn a_limit_takes_the_same_sites_every_run() {
    let files: &[(&str, &str)] = &[(
        "a.py",
        "def alpha_one():\n    return 1\n\n\ndef alpha_two():\n    return 2\n\n\ndef alpha_three():\n    return 3\n",
    )];
    let recipe_text =
        "schema 1\nrecipe r { delete where kind=function name~\"alpha_*\" unused limit 2 }";
    let (tmp_a, first, after_a) = run(files, recipe_text);
    let (tmp_b, second, after_b) = run(files, recipe_text);

    assert_eq!(first.steps[0].matched, 3);
    assert_eq!(first.steps[0].applied, 2);
    assert_eq!(second.steps[0].applied, 2);
    assert_eq!(
        after_a[&tmp_a.path().join("a.py")].1,
        after_b[&tmp_b.path().join("a.py")].1,
        "a limited step must be deterministic or a recipe is not re-runnable"
    );
}

#[test]
fn an_expectation_that_fails_is_reported_rather_than_thrown() {
    let (_tmp, report, _after) = run(
        &[("a.py", AUTH)],
        "schema 1\n\
         recipe r {\n\
           rename to \"verify\" where name=\"modern_auth_check\"\n\
           expect changed > 5 files\n\
         }\n",
    );
    assert!(!report.ok);
    assert_eq!(report.expectations.len(), 1);
    assert!(!report.expectations[0].held);
    assert_eq!(report.expectations[0].actual, "1 files");
}

#[test]
fn no_new_unused_notices_what_a_change_orphaned() {
    // A refactoring that removes a call and orphans a function has not finished, and
    // this is how a recipe says so.
    let (_tmp, report, _after) = run(
        &[(
            "a.py",
            "def helper():\n    return 1\n\n\ndef entry():\n    return helper()\n",
        )],
        "schema 1\n\
         recipe r {\n\
           restructure python 'helper()' => '1'\n\
           expect no-new unused\n\
         }\n",
    );
    assert_eq!(report.expectations.len(), 1);
    assert!(
        !report.expectations[0].held,
        "removing the only call to `helper` orphans it, and the recipe has to notice: \
         {report:?}"
    );
}

#[test]
fn a_refusal_stops_the_run_and_writes_nothing() {
    // `stop` is the default because a step that refused has not done what the recipe says it
    // does.
    let (tmp, report, after) = run(
        &[(
            "a.py",
            "def used():\n    return 1\n\n\ndef entry():\n    return used()\n",
        )],
        "schema 1\nrecipe r { delete where name=\"used\" }",
    );
    assert!(!report.ok);
    assert_eq!(
        after[&tmp.path().join("a.py")].1,
        "def used():\n    return 1\n\n\ndef entry():\n    return used()\n",
        "a stopped run must leave the workspace as it found it"
    );
}

#[test]
fn a_stopped_run_counts_only_the_steps_that_ran() {
    let (_tmp, report, _after) = run(
        &[(
            "a.py",
            "def dead():\n    return 1\n\n\ndef used():\n    return 2\n\n\ndef entry():\n    return used()\n",
        )],
        "schema 1\n\
         recipe r {\n\
           delete where name=\"dead\"\n\
           delete where name=\"used\"\n\
         }\n",
    );
    assert_eq!(report.steps.len(), 2, "got {:?}", report.steps);
    let stopped = report.stopped.as_deref().expect("the second step refused");
    assert!(stopped.contains("step 2 refused"), "got {stopped}");
    assert!(!report.ok);
}

#[test]
fn a_stopped_run_still_reports_the_length_of_the_recipe() {
    // The report headed itself with the steps the run reached, so a three-step recipe that
    // stopped at the second announced itself as a two-step recipe.
    let (_tmp, report, _after) = run(
        &[(
            "a.py",
            "def dead():\n    return 1\n\n\ndef used():\n    return 2\n\n\ndef entry():\n    return used()\n",
        )],
        "schema 1\n\
         recipe r {\n\
           delete where name=\"dead\"\n\
           delete where name=\"used\"\n\
           delete where name=\"gone\" allow-empty\n\
         }\n",
    );
    assert_eq!(report.steps_in_recipe, 3, "the recipe holds three steps");
    assert_eq!(report.steps.len(), 2, "the run reached two of them");
}

/// A misspelled predicate *value* blamed the repository.
#[test]
fn a_misspelled_predicate_value_names_itself() {
    let cases = [
        ("delete where kind=functoin", "functoin", "unknown variant"),
        (
            "delete where lang=pyhton",
            "pyhton",
            "Did you mean `python`",
        ),
    ];

    for (step, typo, expected) in cases {
        let (tmp, sources) = workspace(&[("a.py", "def legacy():\n    return 1\n")]);
        let source = format!("schema 1\n\nrecipe t {{\n  description \"x\"\n  {step}\n}}\n");
        let file = recipe::parse(&source).expect("the recipe parses");
        let err = recipe::run(
            &file.recipes[0],
            sources,
            &Options {
                root: tmp.path(),
                catalogs: &[],
            },
        )
        .expect_err("a value that is not one of the values");
        fun_refactor::vfs::use_filesystem();

        let message = format!("{err:#}");
        assert!(message.contains(typo), "should quote `{typo}`: {message}");
        assert!(
            message.contains(expected),
            "should say what would have worked: {message}."
        );
    }
}

#[test]
fn a_restructure_pattern_that_matches_nothing_stops_the_run() {
    // The pattern is the step's selector, and a selector that chooses nothing stops the recipe.
    let (tmp, sources) = workspace(&[("a.py", "def entry():\n    return modern()\n")]);
    let file =
        recipe::parse("schema 1\nrecipe r { restructure python 'legacy($X)' => 'modern($X)' }")
            .expect("the recipe parses");
    let err = recipe::run(
        &file.recipes[0],
        sources,
        &Options {
            root: tmp.path(),
            catalogs: &[],
        },
    )
    .expect_err("matching nothing is a stop, never a success.");
    fun_refactor::vfs::use_filesystem();
    assert!(
        format!("{err:#}").contains("matched nothing"),
        "the stop names the empty match: {err:#}."
    );
}

#[test]
fn allow_empty_lets_a_conditional_restructure_match_nothing() {
    let (_tmp, report, _after) = run(
        &[("a.py", "def entry():\n    return modern()\n")],
        "schema 1\nrecipe r { restructure python 'legacy($X)' => 'modern($X)' allow-empty }",
    );
    assert!(report.ok, "{report:?}");
    assert_eq!(
        report.steps[0].matched, 0,
        "the count is honest: {report:?}"
    );
    assert_eq!(report.steps[0].applied, 0);
}

#[test]
fn a_restructure_step_counts_occurrences_rather_than_a_constant_one() {
    // Two call sites in one file.
    let (_tmp, report, after) = run(
        &[(
            "a.py",
            "def entry():\n    legacy(1)\n\n    legacy(2)\n    return 0\n",
        )],
        "schema 1\nrecipe r { restructure python 'legacy($X)' => 'modern($X)' }",
    );
    assert_eq!(report.steps[0].matched, 2, "{report:?}");
    assert_eq!(report.steps[0].applied, 2, "{report:?}");
    let (_, text) = after.values().next().expect("one file");
    assert!(
        text.contains("modern(1)") && text.contains("modern(2)"),
        "{text}"
    );
}

const GO_LIB: &str = "package main\n\nimport \"fmt\"\n\n\
                      func Helper(a int) int {\n\treturn a * 2\n}\n\n\
                      func main() {\n\tfmt.Println(Helper(4))\n}\n";

#[test]
fn a_translate_step_writes_the_file_beside_its_source() {
    // The verb that closes the gap between the two halves of this tool.
    let (tmp, report, after) = run(
        &[("lib.go", GO_LIB)],
        "schema 1\nrecipe port {\n  translate to python where lang=go\n}\n",
    );
    assert!(report.ok, "{report:?}");
    let step = &report.steps[0];
    assert_eq!(step.matched, 1);
    assert_eq!(
        step.files_created,
        vec![PathBuf::from("lib.py")],
        "a created file counts apart from a changed one."
    );
    let written = after
        .get(&tmp.path().join("lib.py"))
        .expect("the destination is in what the run left");
    assert_eq!(written.0, fun_refactor::lang::Language::Python);
    assert!(
        written.1.contains("def helper(a: int) -> int:"),
        "{}",
        written.1
    );
    // The source is untouched: a translation is written beside it, never over it.
    assert_eq!(
        after.get(&tmp.path().join("lib.go")).map(|f| f.1.as_str()),
        Some(GO_LIB)
    );
}

#[test]
fn a_later_step_acts_on_what_the_translation_created() {
    // The point of putting translation in the DSL rather than beside it.
    let (tmp, report, after) = run(
        &[("lib.go", GO_LIB)],
        "schema 1\nrecipe port {\n  translate to python where lang=go\n  \
         rename to \"doubled\" where name=\"helper\" kind=function\n}\n",
    );
    assert!(report.ok, "{report:?}");
    assert_eq!(report.steps[1].matched, 1, "the rename found the new file");
    let written = &after
        .get(&tmp.path().join("lib.py"))
        .expect("the destination")
        .1;
    assert!(
        written.contains("def doubled(") && written.contains("print(doubled(4))"),
        "the declaration and its call both moved:\n{written}"
    );
}

#[test]
fn a_translation_that_is_already_there_refuses_and_says_what_a_recipe_can_do() {
    // The standalone command offers `--force` and `--out`.
    let (_tmp, report, _after) = run(
        &[
            ("lib.go", GO_LIB),
            ("lib.py", "def helper(a):\n    return a\n"),
        ],
        "schema 1\nrecipe port {\n  translate to python where lang=go\n}\n",
    );
    assert!(!report.ok);
    let reason = &report.steps[0].refusals[0].reason;
    assert!(
        reason.contains("already there") && reason.contains("on-refusal"),
        "the refusal points at the grammar, not at flags it does not have: {reason}"
    );
}

#[test]
fn what_a_translation_could_not_carry_is_a_warning_against_its_source() {
    // A fidelity note is the same kind of fact as a reference too weak to rewrite: the step
    // succeeded and left something for a person.
    let (_tmp, report, _after) = run(
        &[("pipe.sh", "count() {\n    ls | wc -l\n}\n")],
        "schema 1\nrecipe port {\n  translate to python where lang=bash\n}\n",
    );
    assert!(report.ok, "{report:?}");
    let warning = report.steps[0]
        .warnings
        .iter()
        .find(|w| w.kind == "translation-loss")
        .expect("the pipeline is a loss and is reported");
    assert_eq!(warning.file, PathBuf::from("pipe.sh"));
    assert_eq!(warning.line, 2, "the line the pipeline is written on");
}

#[test]
fn a_language_nothing_can_be_written_as_is_refused_while_the_recipe_is_read() {
    // Before a file is touched.
    let error = recipe::parse("schema 1\nrecipe port {\n  translate to sass where lang=go\n}\n")
        .expect_err("nothing can be written as sass");
    let said = error.to_string();
    assert!(
        said.contains("nothing can be written as sass"),
        "the refusal names the target: {said}"
    );

    let unknown = recipe::parse("schema 1\nrecipe port {\n  translate to cobol where lang=go\n}\n")
        .expect_err("cobol is not in this build");
    assert!(
        unknown
            .to_string()
            .contains("is not a language this build knows"),
        "{unknown}"
    );
}

#[test]
fn translate_needs_a_selector_like_every_other_operation_that_takes_one() {
    let error = recipe::parse("schema 1\nrecipe port {\n  translate to python\n}\n")
        .expect_err("a step with no selector acts on everything");
    assert!(
        error.to_string().contains("needs a `where` clause"),
        "{error}"
    );
}
