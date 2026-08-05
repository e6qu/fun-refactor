//! Moving a symbol, and the imports that have to move with it.
//!
//! A move is behaviour-preserving in the same sense a rename is: the same code runs on
//! the same values, and only where it is written changes. That means the destination
//! has to keep working, which means the imports on both sides have to end up right.

use fun_refactor::index::Index;
use fun_refactor::refactor::move_symbol;
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("writing the file");
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
    // removing it either — and the file failed on the line that used to make it work.
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
    // file — legal in Python, a syntax error in half the other targets, and
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
