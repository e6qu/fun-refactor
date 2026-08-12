//! Every name in a workspace, asked to be a flag.
//!
//! `fr remove-flag` replaces every use of a name with `true` or `false`. Nothing about a name
//! says it held a boolean, and the tests that existed all named a flag that did: every fixture
//! declared `USE_NEW = true` and then checked what the cascade made of it. Sweeping instead,
//! asking for every symbol in a real project, both values, asked the question the fixtures
//! never did. The answers were code no compiler accepts:
//!
//! * `const DocumentScope = @import("DocumentScope.zig")` is a Zig module, and a Zig feature
//!   flag is also a `const`. Removing it wrote `*const true`.
//! * `pub const Position = offsets.Position` is a type, and Zig passes a type as an argument,
//!   so `expectEqualSlices(Position, …)` became `expectEqualSlices(true, …)`.
//! * A flag held by a function is read by calling it, and replacing the callee gave `if
//!   true()`, which then never collapsed either.
//! * A flag whose every use was declined still had its declaration deleted. So a shell script
//!   that read `true` started reading the default in `${USE_NEW:-no}`.
//! * `export async function DELETE(…)` is a Next.js route handler that nothing in the workspace
//!   calls. Removing the "flag" removed the route.
//!
//! What is asserted here is not the text of any answer. It is that the tool either refuses with
//! a reason, or writes something that still parses.

use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;
use fun_refactor::refactor::cascade;
use fun_refactor::scan::{scan, ScanOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ helpers

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    dir
}

/// Every distinct symbol name in a workspace.
fn names_in(root: &Path) -> Vec<String> {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    let index = fun_refactor::index::Index::build_from_scan(&scanned).expect("index");
    let mut names: Vec<String> = index.symbols.iter().map(|s| s.name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// The workspace as the cascade sees it, and as this re-reads it afterwards.
fn sources(root: &Path) -> BTreeMap<PathBuf, (Language, String)> {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    scanned
        .files
        .iter()
        .filter_map(|file| {
            let text = std::fs::read_to_string(&file.path).ok()?;
            Some((file.path.clone(), (file.language, text)))
        })
        .collect()
}

/// The result of a cascade, file by file, with the edits applied in memory.
fn applied(
    before: &BTreeMap<PathBuf, (Language, String)>,
    plan: &cascade::CascadePlan,
) -> Vec<(PathBuf, Language, String)> {
    let mut out = Vec::new();
    for (path, edits) in plan.edits.iter() {
        let Some((language, original)) = before.get(path) else {
            continue;
        };
        let after = fun_refactor::edit::apply_to_string(original, edits).expect("the edits apply");
        out.push((path.clone(), *language, after));
    }
    out
}

fn parses(language: Language, source: &str) -> bool {
    Parsers::new()
        .parse(language, source)
        .map(|parsed| !parsed.has_errors())
        .unwrap_or(false)
}

/// Ask for every name in the workspace, both values, and report what came back.
struct Sweep {
    refused: usize,
    applied: usize,
    /// Every result that stopped parsing, which is the only outcome this forbids.
    broke: Vec<String>,
}

fn sweep(root: &Path) -> Sweep {
    let before = sources(root);
    let already_broken: Vec<&PathBuf> = before
        .iter()
        .filter(|(_, (language, text))| !parses(*language, text))
        .map(|(path, _)| path)
        .collect();
    assert!(
        already_broken.is_empty(),
        "the fixture does not parse before anything touches it: {already_broken:?}"
    );

    let mut result = Sweep {
        refused: 0,
        applied: 0,
        broke: Vec::new(),
    };
    for name in names_in(root) {
        for value in [true, false] {
            match cascade::remove_flag_in(before.clone(), &name, value) {
                Err(_) => result.refused += 1,
                Ok(plan) => {
                    result.applied += 1;
                    for (path, language, text) in applied(&before, &plan) {
                        if !parses(language, &text) {
                            result.broke.push(format!(
                                "remove-flag {name} --value {value} left {} unparseable:\n{text}",
                                path.display()
                            ));
                        }
                    }
                }
            }
        }
    }
    result
}

// -------------------------------------------------------- the fixture corpus

/// One file per language this cascade supports, each holding the shapes the sweep found. A
/// boolean flag, a constant that is not one, something that names a type. A use of the flag the
/// substitution cannot write a literal into.
fn corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "a.rs",
            "const RS_FLAG: bool = true;\nconst RS_NAME: &str = \"hello\";\n\
             const RS_LIMIT: usize = 5;\n\nfn rs_is_on() -> bool {\n    true\n}\n\n\
             fn rs_label() -> String {\n    String::new()\n}\n\n\
             fn rs_run() {\n    if RS_FLAG {\n        rs_new();\n    } else {\n        \
             rs_old();\n    }\n    if rs_is_on() {\n        rs_also();\n    }\n}\n\n\
             fn rs_new() {}\nfn rs_old() {}\nfn rs_also() {}\n",
        ),
        (
            "b.go",
            "package b\n\nconst GoFlag = true\nconst GoName = \"hello\"\nconst GoLimit = 5\n\n\
             func GoIsOn() bool {\n\treturn true\n}\n\n\
             func GoRun() {\n\tif GoFlag {\n\t\tgoNew()\n\t} else {\n\t\tgoOld()\n\t}\n\t\
             if GoIsOn() {\n\t\tgoAlso()\n\t}\n}\n\n\
             func goNew() {}\nfunc goOld() {}\nfunc goAlso() {}\n",
        ),
        (
            "c.zig",
            "const ZigStore = @import(\"store.zig\");\nconst ZigUri = []const u8;\n\
             const ZIG_FLAG = true;\nconst ZIG_NAME = \"hello\";\n\n\
             pub fn zigHold(u: ZigUri, s: ZigStore) void {\n    _ = u;\n    _ = s;\n}\n\n\
             pub fn zigRun() void {\n    if (ZIG_FLAG) {\n        zigNew();\n    } else {\n        \
             zigOld();\n    }\n}\n\npub fn zigNew() void {}\npub fn zigOld() void {}\n",
        ),
        ("store.zig", "pub const zigKind: u8 = 1;\n"),
        (
            "d.ts",
            "const TS_FLAG = true;\nconst TS_NAME = \"hello\";\nconst TS_SETTINGS = { a: 1 };\n\n\
             export function tsIsOn(): boolean {\n  return true;\n}\n\n\
             export function tsLabel(): string {\n  return \"x\";\n}\n\n\
             export function tsRun(): void {\n  if (TS_FLAG) {\n    tsNew();\n  } else {\n    \
             tsOld();\n  }\n  if (tsIsOn()) {\n    tsAlso();\n  }\n}\n\n\
             function tsNew(): void {}\nfunction tsOld(): void {}\nfunction tsAlso(): void {}\n",
        ),
        (
            "e.py",
            "PY_FLAG = True\nPY_NAME = \"hello\"\nPY_SETTINGS = {\"a\": 1}\n\n\n\
             def py_is_on() -> bool:\n    return True\n\n\n\
             def py_label() -> str:\n    return \"x\"\n\n\n\
             def py_run():\n    if PY_FLAG:\n        py_new()\n    else:\n        py_old()\n    \
             if py_is_on():\n        py_also()\n\n\n\
             def py_new():\n    pass\n\n\ndef py_old():\n    pass\n\n\ndef py_also():\n    pass\n",
        ),
        (
            "f.java",
            "class JavaHome {\n  static final boolean JAVA_FLAG = true;\n  \
             static final String JAVA_NAME = \"hello\";\n  static final int JAVA_LIMIT = 5;\n\n  \
             static void javaRun() {\n    if (JAVA_FLAG) {\n      javaNew();\n    } else {\n      \
             javaOld();\n    }\n  }\n\n  static void javaNew() {}\n  static void javaOld() {}\n}\n",
        ),
        (
            "g.sh",
            "SH_FLAG=true\nSH_NAME=\"hello\"\nSH_LIMIT=5\n\n\
             if [ \"$SH_FLAG\" = true ]; then\n  sh_new\nelse\n  sh_old\nfi\n",
        ),
        (
            "h.tf",
            "variable \"tf_enabled\" {\n  type = bool\n}\n\n\
             variable \"tf_label\" {\n  type = string\n}\n\n\
             resource \"aws_s3_bucket\" \"a\" {\n  count = var.tf_enabled ? 1 : 0\n}\n",
        ),
    ]
}

/// The corpus split into one workspace per language, keeping the two Zig files
/// together because one imports the other.
fn by_language() -> Vec<Vec<(&'static str, &'static str)>> {
    let all = corpus();
    let mut zig = Vec::new();
    let mut groups = Vec::new();
    for file in all {
        match file.0.ends_with(".zig") {
            true => zig.push(file),
            false => groups.push(vec![file]),
        }
    }
    groups.push(zig);
    groups
}

/// The name each language's fixture gives to a flag that really is one.
const REAL_FLAGS: [&str; 9] = [
    "RS_FLAG",
    "GoFlag",
    "ZIG_FLAG",
    "TS_FLAG",
    "PY_FLAG",
    "JAVA_FLAG",
    "SH_FLAG",
    "tf_enabled",
    "rs_is_on",
];

#[test]
fn no_name_in_the_corpus_produces_something_that_stops_parsing() {
    // One workspace per language. The cascade re-indexes every file it holds on every
    // round, so asking about a Java constant in a workspace that also holds nine other
    // languages re-parses all of them for nothing.
    let mut result = Sweep {
        refused: 0,
        applied: 0,
        broke: Vec::new(),
    };
    for group in by_language() {
        let tmp = workspace(&group);
        let one = sweep(tmp.path());
        result.refused += one.refused;
        result.applied += one.applied;
        result.broke.extend(one.broke);
    }
    assert!(
        result.broke.is_empty(),
        "{} of {} results stopped parsing:\n\n{}",
        result.broke.len(),
        result.applied,
        result.broke.join("\n\n")
    );
    // A sweep that refused everything would also report nothing broken. So what it carried
    // through is part of the result: every language's real flag has to be one.
    for flag in REAL_FLAGS {
        let tmp = workspace(&corpus());
        cascade::remove_flag_in(sources(tmp.path()), flag, true)
            .unwrap_or_else(|e| panic!("{flag} is a flag and removing it was refused: {e}"));
    }
    eprintln!(
        "remove-flag sweep: {} plans, {} refusals, 0 results that stopped parsing",
        result.applied, result.refused
    );
}

// ------------------------------------------------ what the sweep found, named

/// Ask for one name and take the refusal.
fn refusal_for(root: &Path, name: &str) -> String {
    cascade::remove_flag_in(sources(root), name, true)
        .expect_err(&format!(
            "'{name}' is not a flag and removing it must be refused"
        ))
        .to_string()
}

#[test]
fn a_module_import_is_not_a_flag() {
    let tmp = workspace(&corpus());
    let error = refusal_for(tmp.path(), "ZigStore");
    assert!(error.contains("holds a module"), "{error}");
    assert!(error.contains("Nothing was changed"), "{error}");
}

#[test]
fn a_type_is_not_a_flag() {
    let tmp = workspace(&corpus());
    let error = refusal_for(tmp.path(), "ZigUri");
    assert!(
        error.contains("holds a type") || error.contains("names a type"),
        "{error}"
    );
}

#[test]
fn a_use_in_type_position_settles_what_the_name_is() {
    // Zig passes a type where any other value goes, so `hold(u: Uri, …)` is the only
    // place that says `Uri` is a type. One such use settles it for every other use.
    let tmp = workspace(&[
        (
            "a.zig",
            "const Handle = other.Raw;\npub fn hold(u: Handle) void {\n    _ = u;\n}\n",
        ),
        ("other.zig", "pub const Raw = []const u8;\n"),
    ]);
    let error = refusal_for(tmp.path(), "Handle");
    assert!(error.contains("names a type"), "{error}");
}

#[test]
fn a_constant_holding_something_other_than_a_boolean_is_not_a_flag() {
    let tmp = workspace(&corpus());
    for (name, held) in [
        ("RS_NAME", "holds a string"),
        ("GoLimit", "holds a number"),
        ("TS_SETTINGS", "holds a collection"),
        ("PY_SETTINGS", "holds a collection"),
        ("SH_NAME", "holds a string"),
    ] {
        let error = refusal_for(tmp.path(), name);
        assert!(error.contains(held), "removing {name}: {error}");
    }
}

#[test]
fn a_declared_type_that_is_not_boolean_is_not_a_flag() {
    let tmp = workspace(&corpus());
    for name in ["rs_label", "tsLabel", "py_label", "JAVA_NAME", "JAVA_LIMIT"] {
        let error = refusal_for(tmp.path(), name);
        assert!(error.contains("is not a flag"), "removing {name}: {error}");
    }
}

#[test]
fn a_terraform_variable_of_another_type_is_not_a_flag() {
    let tmp = workspace(&corpus());
    let error = refusal_for(tmp.path(), "tf_label");
    assert!(error.contains("is declared `string`"), "{error}");
}

#[test]
fn a_flag_nothing_reads_is_not_a_flag_removal() {
    // `export async function DELETE(…)` is a route handler nothing in the workspace calls.
    // There is no use to substitute and no conditional to collapse. So what was left was a
    // deletion, which is a different command with different checks.
    let tmp = workspace(&[(
        "route.ts",
        "export async function DELETE(req: Request) {\n  \
         return new Response(null, { status: 204 });\n}\n",
    )]);
    let error = refusal_for(tmp.path(), "DELETE");
    assert!(error.contains("nothing reads it"), "{error}");
    assert!(error.contains("fr delete"), "{error}");
}

#[test]
fn a_flag_held_by_a_function_is_replaced_along_with_its_call() {
    // Replacing the callee alone produced `if true()`, which is not a boolean literal,
    // so the conditional it was supposed to settle never collapsed either.
    let tmp = workspace(&[(
        "a.rs",
        "fn is_on() -> bool {\n    true\n}\n\n\
         fn run() {\n    if is_on() {\n        a();\n    } else {\n        b();\n    }\n}\n\n\
         fn a() {}\nfn b() {}\n",
    )]);
    let plan = cascade::remove_flag_in(sources(tmp.path()), "is_on", true).expect("a plan");
    let out = applied(&sources(tmp.path()), &plan);
    let text = &out.first().expect("one file").2;
    assert!(!text.contains("true()"), "got:\n{text}");
    assert!(text.contains("a();"), "got:\n{text}");
    assert!(!text.contains("b();"), "got:\n{text}");
}

#[test]
fn a_declaration_whose_reader_stayed_stays_too() {
    // The expansion cannot be rewritten, so the assignment that feeds it has to survive:
    // removing it turned a script that read `true` into one that reads `no`.
    let tmp = workspace(&[(
        "run.sh",
        "USE_NEW=true\n\nif [ \"$USE_NEW\" = true ]; then\n  go\nfi\necho \"${USE_NEW:-no}\"\n",
    )]);
    let plan = cascade::remove_flag_in(sources(tmp.path()), "USE_NEW", true).expect("a plan");
    let out = applied(&sources(tmp.path()), &plan);
    let text = &out.first().expect("one file").2;
    assert!(text.contains("USE_NEW=true"), "got:\n{text}");
    assert!(
        plan.unfinished
            .iter()
            .any(|u| u.contains("the declaration stayed")),
        "{:?}",
        plan.unfinished
    );
}

#[test]
fn a_cascade_that_changes_nothing_says_so_instead_of_reporting_a_plan() {
    let tmp = workspace(&[("run.sh", "USE_NEW=true\n\necho \"${USE_NEW:-no}\"\n")]);
    let error = refusal_for(tmp.path(), "USE_NEW");
    assert!(error.contains("could be removed"), "{error}");
    assert!(error.contains("not a plain expansion"), "{error}");
}

// ------------------------------------------------- against the vendored corpus

/// The same questions against code somebody shipped.
///
/// The fixtures above are written by the person writing the assertion. These names come
/// from `tests/corpus`, vendored unmodified and pinned; see `tests/corpus/PROVENANCE.md`.
#[test]
fn the_vendored_corpus_answers_the_same_way() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let before = sources(&root);

    for (name, expected) in [
        ("DocumentScope", "holds a module"),
        ("Position", "names a type"),
        ("UPPER_CAMEL_CASE", "is an enum constant"),
        ("DELETE", "nothing reads it"),
    ] {
        let error = cascade::remove_flag_in(before.clone(), name, true)
            .expect_err(&format!("'{name}' is not a flag"))
            .to_string();
        assert!(
            error.contains(expected),
            "removing {name} should have said `{expected}`:\n{error}"
        );
    }
}
