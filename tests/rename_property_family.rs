//! A Python property is one attribute with two doors, and both doors rename.
//!
//! `@property def size` and `@size.setter def size` declare one thing. Renaming
//! the getter alone left the setter answering the old name, left `@size.setter`
//! reading a binding the class no longer had, and left `b.size` callers behind
//! because the getter and setter counted as two candidates and the use site was
//! called ambiguous inside the very class that declares it.

use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn symbol_at(
    index: &Index,
    path: &Path,
    source: &str,
    needle: &str,
) -> fun_refactor::model::SymbolId {
    let offset = source.find(needle).expect("the needle") + needle.len() - 1;
    index
        .symbols
        .iter()
        .find(|s| s.file == path && s.name_span.contains_offset(offset))
        .expect("a symbol at the needle")
        .id
}

fn applied(index_root: &Path, file: &str, plan: &rename::RenamePlan) -> String {
    let path = index_root.join(file);
    let before = std::fs::read_to_string(&path).unwrap();
    match plan.edits.edits_for(&path) {
        Some(edits) => fun_refactor::edit::apply_to_string(&before, edits).unwrap(),
        None => before,
    }
}

const BOX_PY: &str = "class Box:\n    def __init__(self) -> None:\n        self._size = 0\n\n    \
    @property\n    def size(self) -> int:\n        return self._size\n\n    \
    @size.setter\n    def size(self, value: int) -> None:\n        self._size = value\n\n\n\
    def use(b: Box) -> int:\n    b.size = 3\n    return b.size\n";

#[test]
fn the_getter_the_setter_and_the_decorator_rename_together() {
    let (tmp, index) = workspace(&[("box.py", BOX_PY)]);
    let id = symbol_at(&index, &tmp.path().join("box.py"), BOX_PY, "def size");
    let plan = rename::plan(&index, id, "width").unwrap();
    let out = applied(tmp.path(), "box.py", &plan);
    assert!(
        out.contains("def width(self) -> int:") && out.contains("def width(self, value: int)"),
        "both defs rename.\n{out}"
    );
    assert!(
        out.contains("@width.setter"),
        "the decorator reads the class-namespace binding and follows it.\n{out}"
    );
    assert!(
        !out.contains("def size") && !out.contains("@size."),
        "no door keeps the old name.\n{out}"
    );
}

#[test]
fn a_receiver_declared_the_owning_class_renames_with_the_property() {
    let (tmp, index) = workspace(&[("box.py", BOX_PY)]);
    let id = symbol_at(&index, &tmp.path().join("box.py"), BOX_PY, "def size");
    let plan = rename::plan(&index, id, "width").unwrap();
    let out = applied(tmp.path(), "box.py", &plan);
    assert!(
        out.contains("b.width = 3") && out.contains("return b.width"),
        "`b` is declared `Box`, and `Box` declares the property.\n{out}"
    );
}

#[test]
fn a_receiver_declared_a_subtype_of_the_owner_renames_too() {
    // `s` is declared `Sub2`, which declares no `area` of its own; the one it
    // reaches is `Base`'s. The owners of a family include everything below them.
    let source = "class Base:\n    def area(self) -> int:\n        return 0\n\n\n\
        class Sub2(Base):\n    pass\n\n\n\
        def measure(s: Sub2) -> int:\n    return s.area()\n";
    let (tmp, index) = workspace(&[("shapes.py", source)]);
    let id = symbol_at(&index, &tmp.path().join("shapes.py"), source, "def area");
    let plan = rename::plan(&index, id, "surface").unwrap();
    let out = applied(tmp.path(), "shapes.py", &plan);
    assert!(
        out.contains("return s.surface()"),
        "a declared subtype receiver reaches the family.\n{out}"
    );
}

#[test]
fn an_attribute_follows_into_a_subclass_declared_in_another_file() {
    // `Sub(Base)` lives one import away from `Base`. The attribute family
    // crosses the declared chain wherever the files sit; `self.count` in the
    // subclass is the same attribute `__init__` declares.
    let base = "class Base:\n    def __init__(self) -> None:\n        self.count = 0\n";
    let sub = "from base import Base\n\n\nclass Sub(Base):\n    def bump(self) -> None:\n        \
        self.count += 1\n";
    let (tmp, index) = workspace(&[("base.py", base), ("sub.py", sub)]);
    let id = symbol_at(&index, &tmp.path().join("base.py"), base, "self.count");
    let plan = rename::plan(&index, id, "total").unwrap();
    let out = applied(tmp.path(), "sub.py", &plan);
    assert!(
        out.contains("self.total += 1"),
        "the subclass site follows across the import.\n{out}"
    );
}

#[test]
fn a_java_var_receiver_takes_its_type_from_the_construction() {
    // `var b = new B()` writes the type on the right of the `=`. The call
    // renames with `B`'s method and `A`'s same-named one stays.
    let source = "class B {\n    int size(int n) {\n        return n * 2;\n    }\n}\n\n\
        class A {\n    int size(int n) {\n        return n * 3;\n    }\n}\n\n\
        class Shop {\n    int run() {\n        var b = new B();\n        return b.size(2);\n    }\n}\n";
    let (tmp, index) = workspace(&[("Shop.java", source)]);
    let id = symbol_at(&index, &tmp.path().join("Shop.java"), source, "int size");
    let plan = rename::plan(&index, id, "grow").unwrap();
    let out = applied(tmp.path(), "Shop.java", &plan);
    assert!(
        out.contains("return b.grow(2);"),
        "the constructed type carries to the call site.\n{out}"
    );
    assert!(
        out.contains("return n * 3;") && out.matches("int size(int n)").count() == 1,
        "`A`'s own `size` keeps its name.\n{out}"
    );
}

#[test]
fn an_unrelated_class_with_the_same_property_name_stays_put() {
    let other = "class Crate:\n    @property\n    def size(self) -> int:\n        return 9\n\n\n\
        def peek(c: Crate) -> int:\n    return c.size\n";
    let (tmp, index) = workspace(&[("box.py", BOX_PY), ("crate_.py", other)]);
    let id = symbol_at(&index, &tmp.path().join("box.py"), BOX_PY, "def size");
    let plan = rename::plan(&index, id, "width").unwrap();
    let out = applied(tmp.path(), "crate_.py", &plan);
    assert!(
        out.contains("c.size") && out.contains("def size"),
        "`c` is declared `Crate`, which is not what is being renamed.\n{out}"
    );
}
