//! Comparing two strings, where four languages agree and two do not.
//!
//! Every writer rendered `==` as `==`. Rust, Go, Python and TypeScript compare the
//! contents of a string that way and so did the source. Java compares references. Zig
//! will not compile it at all.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, to: Language) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("a.rs");
    std::fs::write(&path, source).expect("the file");
    transpile::plan(&path, to).expect("a translation").output
}

const STRINGS: &str = "\
pub fn same(a: String, b: String) -> bool {
    return a == b;
}

pub fn different(a: String, b: String) -> bool {
    return a != b;
}

pub fn numbers(a: i64, b: i64) -> bool {
    return a == b;
}
";

#[test]
fn java_compares_the_contents_of_a_string() {
    // `a == b` on a Java String asks whether they are the same object, which is false
    // for two equal strings that were built and not interned. `Objects.equals`
    // and not `a.equals(b)`, because it answers for null on either side instead of
    // throwing, and written out in full because this writer emits no imports.
    let out = translated(STRINGS, Language::Java);
    assert!(
        out.contains("return java.util.Objects.equals(a, b);"),
        "{out}"
    );
    assert!(
        out.contains("return !java.util.Objects.equals(a, b);"),
        "{out}"
    );
    // Numbers are compared with `==` in Java as everywhere else.
    assert!(out.contains("return a == b;"), "{out}");
}

#[test]
fn zig_compares_the_contents_of_a_string() {
    // A Zig string is a `[]const u8`, and `==` on a slice is not something the compiler
    // accepts. The output looked like the other five and did not build.
    let out = translated(STRINGS, Language::Zig);
    assert!(out.contains("return std.mem.eql(u8, a, b);"), "{out}");
    assert!(out.contains("return !std.mem.eql(u8, a, b);"), "{out}");
    assert!(out.contains("return a == b;"), "{out}");
}

#[test]
fn zig_binds_the_standard_library_it_reaches_for() {
    // `std.mem.eql` names something the file has to bind first, and nothing did.
    let out = translated(STRINGS, Language::Zig);
    assert!(out.contains("const std = @import(\"std\");"), "{out}");
}

#[test]
fn zig_binds_nothing_it_does_not_need() {
    let out = translated(
        "pub fn n(a: i64, b: i64) -> bool {\n    return a == b;\n}\n",
        Language::Zig,
    );
    assert!(!out.contains("@import"), "{out}");
}

#[test]
fn the_languages_that_already_agreed_are_untouched() {
    // Go's `==` on a string compares contents, and so do Python's and TypeScript's.
    // A rewrite here would be a change with no defect behind it.
    for (target, expected) in [
        (Language::Go, "return a == b"),
        (Language::Python, "return a == b"),
        (Language::TypeScript, "return a === b"),
    ] {
        let out = translated(STRINGS, target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn zig_refuses_to_join_two_strings_rather_than_pretending() {
    // Joining two slices in Zig means allocating, and the allocator is a parameter the
    // function does not have — inventing one changes the signature every caller was
    // written against. `a + b` on two slices is not something the compiler accepts, so
    // it went out looking like the other four targets and not building.
    //
    // `@compileError` is a value anywhere one is expected, it says why in the
    // compiler's own output, and it cannot be mistaken for code that works — which
    // returning an empty slice quietly could. Zig has no block comment, so a marker
    // beside the value would swallow the rest of the line.
    let out = translated(
        "pub fn join(a: String, b: String) -> String {\n    return a + b;\n}\n",
        Language::Zig,
    );
    assert!(out.contains("@compileError("), "{out}");
    assert!(out.contains("allocator"), "{out}");
}

#[test]
fn zig_still_adds_two_numbers() {
    let out = translated(
        "pub fn add(a: i64, b: i64) -> i64 {\n    return a + b;\n}\n",
        Language::Zig,
    );
    assert!(out.contains("return a + b;"), "{out}");
    assert!(!out.contains("@compileError"), "{out}");
}

#[test]
fn the_languages_that_join_strings_with_plus_are_untouched() {
    // Java, Go, Python and TypeScript all concatenate with `+`. Only Zig does not.
    let source = "pub fn join(a: String, b: String) -> String {\n    return a + b;\n}\n";
    for target in [
        Language::Java,
        Language::Go,
        Language::Python,
        Language::TypeScript,
    ] {
        let out = translated(source, target);
        assert!(out.contains("return a + b"), "{target}:\n{out}");
    }
}
