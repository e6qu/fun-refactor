//! What the moved code needed where it came from.
//!
//! A move rewrites the callers, which is the half everyone thinks of. The other half is
//! that the moved code itself referred to things, and half of those live in the file it
//! just left. The generic path writes an import pointing back for them; the
//! per-language paths looked only at what the source file *imported*, never at what it
//! *declared*.

use fun_refactor::index::Index;
use fun_refactor::refactor::move_symbol;
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

struct Moved {
    files: Vec<(PathBuf, String)>,
    warnings: Vec<String>,
}

impl Moved {
    fn file(&self, name: &str) -> &str {
        self.files
            .iter()
            .find(|(path, _)| path.ends_with(name))
            .map(|(_, text)| text.as_str())
            .unwrap_or_else(|| panic!("no {name} in the result"))
    }
}

fn moved(files: &[(&str, &str)], symbol: &str, destination: &str) -> Moved {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(path, content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == symbol)
        .unwrap_or_else(|| panic!("no `{symbol}`"))
        .id;
    let plan =
        move_symbol::to_file(&index, id, &tmp.path().join(destination)).expect("a move plan");

    let mut out = Vec::new();
    for (name, content) in files {
        let path = tmp.path().join(name);
        let text = match plan.edits.edits_for(&path) {
            Some(edits) => fun_refactor::edit::apply_to_string(content, edits).expect("applying"),
            None => (*content).to_string(),
        };
        out.push((path, text));
    }
    Moved {
        files: out,
        warnings: plan.warnings.clone(),
    }
}

const SELF_REFERRING: &[(&str, &str)] = &[
    (
        "counters.py",
        "LIMIT = 100\n\n\nclass Counter:\n    STEP = 5\n\n    def bump(self, n):\n        \
         return min(n + Counter.STEP, LIMIT)\n",
    ),
    ("models.py", "NAME = \"models\"\n"),
    (
        "use.py",
        "from counters import Counter\n\nprint(Counter().bump(1))\n",
    ),
];

/// `Counter.STEP` inside `Counter`'s own method travels with the class. Counting it as a
/// use left behind had the source importing a name it no longer mentions. The moved code
/// also needs `LIMIT`, which the source keeps. The two phantom imports then read as a
/// cycle, and the move was refused for one that does not exist.
#[test]
fn a_class_that_names_itself_leaves_no_use_behind() {
    let result = moved(SELF_REFERRING, "Counter", "models.py");
    let counters = result.file("counters.py");
    assert!(
        !counters.contains("Counter"),
        "the source keeps no use of the moved class:\n{counters}"
    );
    let models = result.file("models.py");
    assert!(models.contains("from counters import LIMIT"), "{models}");
    assert!(models.contains("class Counter:"), "{models}");
    assert!(
        result.file("use.py").contains("from models import Counter"),
        "{}",
        result.file("use.py")
    );
}

#[test]
fn a_rust_move_carries_what_the_source_file_defined() {
    // `cargo check` on the old output: `cannot find value PI in this scope`, with rustc
    // suggesting the exact `use` the tool should have written. Nothing warned.
    let result = moved(
        &[
            (
                "src/a.rs",
                "pub const PI: f64 = 3.14;\nconst SCALE: f64 = 2.0;\n\n\
                 pub fn area(r: f64) -> f64 {\n    PI * r * r * SCALE\n}\n",
            ),
            ("src/b.rs", "pub fn other() -> f64 { 1.0 }\n"),
            ("src/main.rs", "mod a;\nmod b;\nfn main() {}\n"),
            ("Cargo.toml", "[package]\nname=\"p\"\nversion=\"0.1.0\"\n"),
        ],
        "area",
        "src/b.rs",
    );
    let b = result.file("b.rs");
    assert!(b.contains("use crate::a::{PI, SCALE};"), "{b}");

    // A private item is invisible from another module, so the `use` alone would not
    // compile. `PI` was already public and is left as it was.
    let a = result.file("a.rs");
    assert!(a.contains("pub const SCALE: f64 = 2.0;"), "{a}");
    assert!(a.contains("pub const PI: f64 = 3.14;"), "{a}");
    assert!(!a.contains("pub pub"), "{a}");
}

#[test]
fn a_zig_move_says_what_it_could_not_carry() {
    // Zig imports a module and qualifies, instead of binding a name, so there is no
    // import to write, the reference itself would have to change. Saying so is the
    // least this can do, and it is infinitely more than saying nothing.
    let result = moved(
        &[
            (
                "a.zig",
                "pub const PI: f64 = 3.14;\n\npub fn area(r: f64) f64 {\n    \
                 return PI * r * r;\n}\n",
            ),
            ("b.zig", "pub fn other() f64 { return 1.0; }\n"),
        ],
        "area",
        "b.zig",
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("PI")),
        "got {:?}",
        result.warnings
    );
}

#[test]
fn a_go_move_inside_one_package_needs_nothing() {
    // One package is one scope. Warning here would be noise about a name that still
    // resolves perfectly well.
    let result = moved(
        &[
            (
                "a.go",
                "package main\n\nconst PI = 3.14\n\nfunc Area(r float64) float64 {\n\t\
                 return PI * r * r\n}\n",
            ),
            (
                "b.go",
                "package main\n\nfunc Other() float64 { return 1.0 }\n",
            ),
        ],
        "Area",
        "b.go",
    );
    assert!(result.warnings.is_empty(), "got {:?}", result.warnings);
}

#[test]
fn a_move_that_needs_nothing_carries_nothing() {
    let result = moved(
        &[
            (
                "src/a.rs",
                "pub fn area(r: f64) -> f64 {\n    3.14 * r * r\n}\n",
            ),
            ("src/b.rs", "pub fn other() -> f64 { 1.0 }\n"),
            ("src/main.rs", "mod a;\nmod b;\nfn main() {}\n"),
            ("Cargo.toml", "[package]\nname=\"p\"\nversion=\"0.1.0\"\n"),
        ],
        "area",
        "src/b.rs",
    );
    assert!(
        !result.file("b.rs").contains("use crate::a"),
        "{}",
        result.file("b.rs")
    );
    assert!(result.warnings.is_empty(), "got {:?}", result.warnings);
}
