//! Renaming one of several things that share a name.
//!
//! This tool resolves references by name and by scope; it does not know types. Where a
//! name belongs to exactly one thing that is no limitation at all, and where it belongs
//! to several it is the whole question. Java overloads `add(int)` beside `add(String)`;
//! two classes in one file each declare `run`; a parameter called `session` appears in
//! four different functions. Each of those broke something different.

use fun_refactor::index::Index;
use fun_refactor::model::Confidence;
use fun_refactor::refactor;
use fun_refactor::scan::ScanOptions;
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the parent directory");
        }
        std::fs::write(&path, content).expect("writing the file");
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn index_of(root: &Path) -> Index {
    Index::build(root, &ScanOptions::default()).expect("an index")
}

/// The symbol whose definition sits at `name` in `file`, `nth` occurrence first.
fn symbol_at(index: &Index, file: &str, name: &str, nth: usize) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.file.ends_with(file))
        .nth(nth)
        .unwrap_or_else(|| panic!("no `{name}` #{nth} in {file}"))
        .id
}

#[test]
fn a_call_on_another_object_is_not_the_method_beside_it() {
    // `c.run(1)` names a member of whatever `c` is. The lexical scope chain has nothing
    // to say about that, and it was answering anyway — with `Exact`, by picking the
    // same-named method in an enclosing scope. Renaming that method rewrote the call.
    let source = "class C:\n    def run(self, a): return a\n\n\n\
                  class D:\n    def run(self, a): return a + 1\n    \
                  def use(self, c): return c.run(1) + self.run(2)\n";
    let (_tmp, root) = workspace(&[("p.py", source)]);
    let index = index_of(&root);
    let d_run = symbol_at(&index, "p.py", "run", 1);
    let references = index.references_to(d_run);
    assert_eq!(references.len(), 1, "{references:?}");
    // The one that survives is `self.run`, which does mean a member of this type.
    assert_eq!(references[0].confidence, Confidence::Exact);

    let plan = refactor::rename::plan(&index, d_run, "go").expect("a rename");
    assert!(
        plan.warnings.iter().any(|w| w.detail.contains("'run'")),
        "the call it left behind has to be reported: {:?}",
        plan.warnings
    );
}

#[test]
fn an_overload_set_is_not_resolved_by_proximity() {
    // Proximity is evidence for a binding and not for a callable. `let x` twice in one
    // block is shadowing and the nearer one is the answer; two methods in one class body
    // are an overload set and the nearer one is a coin flip. Both bare `add(...)` calls
    // resolved to whichever was written second, at `Exact` — so renaming that one
    // rewrote calls belonging to the other.
    let source = "public class A {\n    public int add(int a) { return a; }\n    \
                  public int add(String s) { return 1; }\n    \
                  public int use() { return add(1) + add(\"x\"); }\n}\n";
    let (_tmp, root) = workspace(&[("A.java", source)]);
    let index = index_of(&root);
    for nth in 0..2 {
        let add = symbol_at(&index, "A.java", "add", nth);
        for reference in index.references_to(add) {
            assert_ne!(
                reference.confidence,
                Confidence::Exact,
                "an overloaded call cannot be resolved exactly: {reference:?}"
            );
        }
    }
}

#[test]
fn a_rename_that_leaves_a_call_behind_says_so() {
    // The rename went through, the calls stayed behind, and the report said nothing at
    // all — because the guess had landed on the *other* symbol, so it was skipped in
    // silence. A weak resolution is a guess wherever it lands.
    let source = "public class A {\n    public int add(int a) { return a; }\n    \
                  public int add(String s) { return 1; }\n    \
                  public int use() { return add(1) + add(\"x\"); }\n}\n";
    let (_tmp, root) = workspace(&[("A.java", source)]);
    let index = index_of(&root);
    let add = symbol_at(&index, "A.java", "add", 0);
    let plan = refactor::rename::plan(&index, add, "plus").expect("a rename");
    assert_eq!(
        plan.warnings.len(),
        2,
        "both calls have to be reported: {:?}",
        plan.warnings
    );
}

#[test]
fn a_name_used_by_another_function_is_not_a_collision() {
    // A parameter is written outside the body it belongs to, so the scope it falls in is
    // the one *around* its function — which is the file. Every parameter of every
    // function therefore shared a scope, and renaming one to a name used by an unrelated
    // function was refused. Measured over the vendored corpora, that was most of the
    // renames a real file offers.
    let source = "def one(session: int) -> int:\n    return session\n\n\n\
                  def two(email: int) -> int:\n    return email\n";
    let (_tmp, root) = workspace(&[("c.py", source)]);
    let index = index_of(&root);
    let session = symbol_at(&index, "c.py", "session", 0);
    refactor::rename::plan(&index, session, "email").expect("no collision between two functions");
}

#[test]
fn a_name_used_in_the_same_function_is_still_a_collision() {
    let source = "def one(session: int, other: int) -> int:\n    return session\n";
    let (_tmp, root) = workspace(&[("b.py", source)]);
    let index = index_of(&root);
    let session = symbol_at(&index, "b.py", "session", 0);
    assert!(
        refactor::rename::plan(&index, session, "other").is_err(),
        "two parameters of one function collide"
    );
}
