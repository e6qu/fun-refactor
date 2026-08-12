//! A call written through a qualifier resolves to what the qualifier names.
//!
//! Three languages spell the same idea three ways, and each was resolved by a rule of its own
//! until one of them had none. Go qualifies with a package, which is a directory. Zig qualifies
//! with an import binding, which is a file. Java qualifies with the type's own name. Rust
//! writes that last one `Type::m` and was the only spelling recognised. So `Widths.width(…)`
//! fell through to the rule that asks which declaration sits nearest, and answered, inside
//! `Holder.java`, with `Holder`'s own method, at exact confidence.
//!
//! What every one of these has in common is that the source wrote the qualifier down. Nothing
//! here is inference: the resolution is as strong as the statement, and that is why these come
//! back `exact` or `import-qualified` and not `field-based`. A refactoring rewrites the first
//! two and refuses the third. So the difference is the difference between a rename that works
//! and one that leaves the callers behind.

use fun_refactor::index::Index;
use fun_refactor::model::{Confidence, SymbolId};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn index_of(root: &Path) -> Index {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    Index::build_from_scan(&scanned).expect("index")
}

/// The one symbol of that name and qualifier.
fn symbol(index: &Index, name: &str, qualifier: Option<&str>) -> SymbolId {
    let found: Vec<&fun_refactor::model::Symbol> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.qualifier.as_deref() == qualifier)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected one {qualifier:?}::{name}, found {}",
        found.len()
    );
    found[0].id
}

/// Every reference to a symbol, as file name and confidence.
fn references(index: &Index, id: SymbolId) -> Vec<(String, Confidence)> {
    let mut out: Vec<(String, Confidence)> = index
        .references_to(id)
        .into_iter()
        .map(|r| {
            (
                r.file
                    .file_name()
                    .expect("a file name")
                    .to_string_lossy()
                    .to_string(),
                r.confidence,
            )
        })
        .collect();
    out.sort();
    out
}

// -------------------------------------------------------------------- Zig

#[test]
fn a_zig_call_through_an_import_binding_resolves_into_that_file() {
    let (_tmp, root) = workspace(&[
        (
            "holder.zig",
            "pub const Holder = struct {\n    items: []const u8,\n\n    \
             pub fn width(self: Holder, n: usize) usize {\n        \
             return self.items.len + n;\n    }\n};\n\n\
             pub fn width(items: []const u8, n: usize) usize {\n    return items.len + n;\n}\n",
        ),
        (
            "util.zig",
            "const holder = @import(\"holder.zig\");\n\n\
             pub fn describe(h: holder.Holder) usize {\n    \
             return holder.width(h.items, 1);\n}\n",
        ),
    ]);
    let index = index_of(&root);

    // The free function and the method share a name, which made the name-matching
    // rules answer "either" and resolve neither.
    let free = symbol(&index, "width", None);
    let method = symbol(&index, "width", Some("Holder"));

    assert_eq!(
        references(&index, free),
        vec![("util.zig".to_string(), Confidence::ImportQualified)],
        "the import statement names the file, so the call is as certain as that statement"
    );
    assert!(
        references(&index, method).is_empty(),
        "nothing calls the method here, and the qualified call is not it"
    );
}

#[test]
fn a_zig_import_path_is_a_file_beside_this_one() {
    // `@import("holder.zig")` has no `./` in front of it, and every path rule assumed one or a
    // dotted module name. So the extension was read as the last segment and the import was
    // looked up as a file called `zig`.
    let (_tmp, root) = workspace(&[
        ("a.zig", "pub const value: u8 = 1;\n"),
        (
            "b.zig",
            "const a = @import(\"a.zig\");\npub fn read() u8 {\n    return a.value;\n}\n",
        ),
    ]);
    let index = index_of(&root);
    assert_eq!(
        references(&index, symbol(&index, "value", None)),
        vec![("b.zig".to_string(), Confidence::ImportQualified)]
    );
}

#[test]
fn a_zig_call_through_a_value_is_not_a_call_through_an_import() {
    // A local bound to a call result. The receiver names no file, no type and no enclosing
    // instance. So nothing about it is written down and the answer stays where it was: a member
    // matched by name, and no stronger.
    //
    // `self` is a different matter and resolves exactly. It is the enclosing instance, which
    // the declaration states.
    let (_tmp, root) = workspace(&[(
        "a.zig",
        "pub const Holder = struct {\n    items: []const u8,\n\n    \
         pub fn size(self: Holder) usize {\n        return self.items.len;\n    }\n};\n\n\
         pub fn make() Holder {\n    return Holder{ .items = \"\" };\n}\n\n\
         pub fn read() usize {\n    const h = make();\n    return h.items.len;\n}\n",
    )]);
    let index = index_of(&root);
    let found = references(&index, symbol(&index, "items", Some("Holder")));

    assert!(
        found.contains(&("a.zig".to_string(), Confidence::Exact)),
        "`self.items` states its own receiver: {found:?}"
    );
    assert!(
        found.contains(&("a.zig".to_string(), Confidence::FieldBased)),
        "`h.items` does not, and must not be promoted by these rules: {found:?}"
    );
}

// ------------------------------------------------------------------- Java

#[test]
fn a_java_static_call_resolves_to_the_class_it_names() {
    let (_tmp, root) = workspace(&[
        (
            "Widths.java",
            "public class Widths {\n    public static int width(byte[] items, int n) {\n        \
             return items.length + n;\n    }\n}\n",
        ),
        (
            "Holder.java",
            "public class Holder {\n    public byte[] items = new byte[0];\n\n    \
             public int width(int n) {\n        return Widths.width(items, n);\n    }\n}\n",
        ),
        (
            "Main.java",
            "public class Main {\n    public static void main(String[] args) {\n        \
             System.out.println(Widths.width(new byte[0], 1));\n    }\n}\n",
        ),
    ]);
    let index = index_of(&root);
    let static_method = symbol(&index, "width", Some("Widths"));
    let instance_method = symbol(&index, "width", Some("Holder"));

    assert_eq!(
        references(&index, static_method),
        vec![
            ("Holder.java".to_string(), Confidence::Exact),
            ("Main.java".to_string(), Confidence::Exact),
        ],
        "`Widths.` says which method this is, in both files"
    );
    // The call inside Holder.java is the one that matters most: the nearest declaration of that
    // name is Holder's own method. That is the answer this used to give.
    assert!(
        references(&index, instance_method).is_empty(),
        "Holder's own method is not what `Widths.width(…)` calls"
    );
}

#[test]
fn one_design_written_in_two_languages_keeps_its_calls_apart() {
    // The qualifier says which type; the file says which language. Matching on the
    // qualifier alone made a workspace holding the same design twice look ambiguous, and
    // a call that had always resolved stopped resolving at all.
    let (_tmp, root) = workspace(&[
        (
            "Money.java",
            "public class Money {\n    public static Money of(int units) {\n        \
             return new Money();\n    }\n\n    public Money plus(int units) {\n        \
             return Money.of(units);\n    }\n}\n",
        ),
        (
            "money.py",
            "class Money:\n    @staticmethod\n    def of(units):\n        return Money()\n\n    \
             def plus(self, units):\n        return Money.of(units)\n",
        ),
    ]);
    let index = index_of(&root);
    for file in ["Money.java", "money.py"] {
        let id = index
            .symbols
            .iter()
            .find(|s| {
                s.name == "of"
                    && s.qualifier.as_deref() == Some("Money")
                    && s.file.file_name().is_some_and(|n| n == file)
            })
            .unwrap_or_else(|| panic!("no Money::of in {file}"))
            .id;
        assert_eq!(
            references(&index, id),
            vec![(file.to_string(), Confidence::Exact)],
            "{file}'s `Money.of(…)` resolves to its own declaration and no other"
        );
    }
}

// --------------------------------------------------------------------- Go

#[test]
fn a_go_call_into_another_package_resolves_there() {
    // The rule this one needs already exists; it is here so the three spellings of one idea are
    // checked in one place. A change to any of them fails beside the others.
    let (_tmp, root) = workspace(&[
        ("go.mod", "module gate\n\ngo 1.21\n"),
        (
            "holder/holder.go",
            "package holder\n\ntype Holder struct {\n\tItems []byte\n}\n\n\
             func Width(items []byte, n int) int {\n\treturn len(items) + n\n}\n\n\
             func (h *Holder) Width(n int) int {\n\treturn Width(h.Items, n)\n}\n",
        ),
        (
            "util/util.go",
            "package util\n\nimport \"gate/holder\"\n\n\
             func Describe(h *holder.Holder) int {\n\treturn holder.Width(h.Items, 1)\n}\n",
        ),
    ]);
    let index = index_of(&root);
    let found = references(&index, symbol(&index, "Width", None));
    assert!(
        found.contains(&("util.go".to_string(), Confidence::ImportQualified)),
        "the cross-package call resolves through the import: {found:?}"
    );
}
