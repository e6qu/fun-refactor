//! Moving a symbol, and the imports that have to move with it.
//!
//! A move is behaviour-preserving in the same sense a rename is. The same code runs on the same
//! values, and only where it is written changes. That means the destination has to keep
//! working, which means the imports on both sides have to end up right.

use fun_refactor::index::Index;
use fun_refactor::refactor::move_symbol;
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("the directory");
        }
        std::fs::write(&path, content).expect("writing the file");
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn moved(files: &[(&str, &str)], symbol: &str, from: &str, to: &str) -> (String, String) {
    let (_tmp, root) = workspace(files);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == symbol && s.file.ends_with(from))
        .unwrap_or_else(|| panic!("no `{symbol}` in {from}"))
        .id;
    let plan = move_symbol::to_file(&index, id, &root.join(to)).expect("a move");
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("a valid plan");
    fun_refactor::edit::commit(&outcomes).expect("writing");
    (
        std::fs::read_to_string(root.join(from)).expect("the source"),
        std::fs::read_to_string(root.join(to)).expect("the destination"),
    )
}

#[test]
fn the_destination_stops_importing_what_it_now_defines() {
    // `from .b import area` in the file `area` is moving *into* points at a file which
    // no longer defines the name. Nothing was adding that import, so nothing was
    // removing it either, and the file failed on the line that used to make it work.
    let (_from, to) = moved(
        &[
            (
                "b.py",
                "import math\n\n\ndef area(r):\n    return math.pi * r * r\n",
            ),
            (
                "a.py",
                "from .b import area\n\n\ndef label(r):\n    return \"area: \" + str(area(r))\n",
            ),
        ],
        "area",
        "b.py",
        "a.py",
    );
    assert!(!to.contains("from .b import area"), "{to}");
    assert!(to.contains("def area(r):"), "{to}");
}

#[test]
fn an_import_of_several_names_keeps_the_others() {
    // The rest are still over there and still needed.
    let (_from, to) = moved(
        &[
            (
                "b.py",
                "def area(r):\n    return r\n\n\ndef perimeter(r):\n    return r\n",
            ),
            (
                "a.py",
                "from .b import area, perimeter\n\n\ndef label(r):\n    return area(r) + perimeter(r)\n",
            ),
        ],
        "area",
        "b.py",
        "a.py",
    );
    assert!(to.contains("from .b import perimeter"), "{to}");
    assert!(!to.contains("import area"), "{to}");
}

#[test]
fn what_the_moved_code_needs_lands_where_imports_go() {
    // Prepending them to the moved text put an `import` statement in the middle of the
    // file, legal in Python, a syntax error in half the other targets, and
    // wrong-looking in all of them.
    let (_from, to) = moved(
        &[
            (
                "b.py",
                "import math\n\n\ndef area(r):\n    return math.pi * r * r\n",
            ),
            ("a.py", "def label(r):\n    return \"x\"\n"),
        ],
        "area",
        "b.py",
        "a.py",
    );
    let import_at = to.find("import math").expect("the carried import");
    let first_def = to.find("def ").expect("a definition");
    assert!(
        import_at < first_def,
        "the import belongs above the code:\n{to}"
    );
}

#[test]
fn an_aliased_import_repoints_and_keeps_its_alias() {
    // `import { foo as increment } from "./a"` names the moved symbol under
    // the name the rest of the file calls it. Leaving that line while adding a
    // plain `import { foo }` broke the build twice. The old import named a gone
    // export, and the new one bound a name nothing uses.
    let files = [
        (
            "a.ts",
            "export function foo(n: number): number {\n    return n + 1;\n}\n\n\
             export function keep(n: number): number {\n    return n;\n}\n",
        ),
        (
            "b.ts",
            "import { foo as increment, keep } from \"./a\";\n\n\
             export function use(): number {\n    return increment(41) + keep(1);\n}\n",
        ),
        ("c.ts", "export const placeholder = 1;\n"),
    ];
    let (_tmp, root) = workspace(&files);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == "foo" && s.file.ends_with("a.ts"))
        .expect("foo")
        .id;
    let plan = move_symbol::to_file(&index, id, &root.join("c.ts")).expect("a move");
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("a valid plan");
    fun_refactor::edit::commit(&outcomes).expect("writing");
    let importer = std::fs::read_to_string(root.join("b.ts")).expect("the importer");
    assert!(
        importer.contains("import { foo as increment } from './c';"),
        "the alias follows the symbol:\n{importer}"
    );
    assert!(
        importer.contains("import { keep } from './a';"),
        "the stayer keeps its old path:\n{importer}"
    );
    assert!(
        !importer.contains("from \"./a\""),
        "no dangling import of the gone export:\n{importer}"
    );
}

#[test]
fn a_first_import_goes_below_the_docstring_and_the_shebang() {
    // Byte zero is above everything, and some things must stay first. The
    // import demoted a `#!` line to line two, so the script stopped running.
    // It pushed a module docstring into an expression nobody reads, so
    // `__doc__` became `None`.
    let (from, _to) = moved(
        &[
            ("pkg/__init__.py", ""),
            (
                "pkg/a.py",
                "\"\"\"Module A does arithmetic.\"\"\"\n\n\ndef helper(n: int) -> int:\n    \
                 return n * 2\n\n\ndef use(n: int) -> int:\n    return helper(n) + 1\n",
            ),
        ],
        "helper",
        "pkg/a.py",
        "pkg/b.py",
    );
    assert!(
        from.starts_with("\"\"\"Module A does arithmetic.\"\"\""),
        "the docstring is still the first thing in the file.\n{from}"
    );
    assert!(
        from.contains("from .b import helper"),
        "and the import is there, below it.\n{from}"
    );

    let (from, _to) = moved(
        &[
            ("pkg/__init__.py", ""),
            (
                "pkg/a.py",
                "#!/usr/bin/env python3\n\n\ndef helper(n: int) -> int:\n    return n * 2\n\n\n\
                 def use(n: int) -> int:\n    return helper(n) + 1\n",
            ),
        ],
        "helper",
        "pkg/a.py",
        "pkg/b.py",
    );
    assert!(
        from.starts_with("#!/usr/bin/env python3"),
        "a shebang only works on line one.\n{from}"
    );
}

/// A move over a whole workspace, answering with the file named.
fn moved_reading(files: &[(&str, &str)], symbol: &str, from: &str, to: &str, read: &str) -> String {
    let (_tmp, root) = workspace(files);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == symbol && s.file.ends_with(from))
        .unwrap_or_else(|| panic!("no `{symbol}` in {from}"))
        .id;
    let plan = move_symbol::to_file(&index, id, &root.join(to)).expect("a move");
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("a valid plan");
    fun_refactor::edit::commit(&outcomes).expect("writing");
    std::fs::read_to_string(root.join(read)).expect("the file")
}

#[test]
fn an_importer_a_directory_away_has_its_import_narrowed() {
    // With the importer beside the definition this worked. With a parent-relative
    // specifier the old named import stayed beside the new one: `TS2300: Duplicate
    // identifier` and `TS2459`. Two imports of one name is valid syntax, so the
    // reparse guard passed it. The cause was one path join that kept the `..` as a
    // component, so the specifier compared unequal to the very file it names.
    let importer = moved_reading(
        &[
            (
                "src/pricing.ts",
                "export function roundCents(value: number): number {\n  return value;\n}\n\n\
                 export function withTax(p: number): number {\n  return roundCents(p);\n}\n",
            ),
            ("src/money.ts", ""),
            (
                "test/run.ts",
                "import { withTax, roundCents } from \"../src/pricing\";\n\n\
                 console.log(roundCents(1), withTax(1));\n",
            ),
        ],
        "roundCents",
        "src/pricing.ts",
        "src/money.ts",
        "test/run.ts",
    );
    assert!(
        !importer.contains("roundCents } from '../src/pricing'")
            && !importer.contains("roundCents } from \"../src/pricing\""),
        "the old import no longer binds the moved name:\n{importer}"
    );
    assert!(
        importer.contains("withTax") && importer.contains("../src/pricing"),
        "what stayed behind is still imported from there:\n{importer}"
    );
    assert!(
        importer.contains("roundCents") && importer.contains("../src/money"),
        "and the moved name arrives from its new home:\n{importer}"
    );
}

#[test]
fn a_parent_relative_import_resolves_to_the_file_it_names() {
    // The narrowing above rests on this one question being answered right, so it is
    // asked here on its own.
    let (_tmp, root) = workspace(&[
        ("src/pricing.ts", "export const rate = 1;\n"),
        (
            "test/run.ts",
            "import { rate } from \"../src/pricing\";\nconsole.log(rate);\n",
        ),
    ]);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    assert_eq!(
        index.resolve_import_path(&root.join("test/run.ts"), "../src/pricing"),
        Some(root.join("src/pricing.ts")),
    );
}
