//! Restructuring the shapes a language is made of: members, arms and macro bodies.
//!
//! A pattern used to have to be an expression, a statement or a whole item. That leaves
//! out most of what changing a program means. A variant of an enum is a member. So is the
//! arm that goes with it. So are a field of a struct, a case of a switch and a name in an
//! or-pattern. None of them parses on its own.
//!
//! A member is written with the separator that puts it in its list, `Scss,`. Most
//! grammars leave that separator out of the member's own node. So the match takes the
//! target's separator with it, and rewriting `Scss,` as two variants leaves two commas
//! rather than three.
//!
//! Rust macros are the other half. `matches!(l, A | B)` holds an or-pattern that
//! tree-sitter cannot parse as one, because no grammar knows what a macro does with its
//! arguments. What is there is a run of tokens, and a shape written in Rust has a run of
//! tokens too.

use fun_refactor::edit::{apply_to_string, plan, Validation};
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::refactor::restructure;
use fun_refactor::scan::{scan, ScanOptions};

/// Rewrite a one-file workspace, and return (matches, rewritten text).
///
/// Every rewrite goes through the same `ReparseStrict` gate the CLI uses, so a member
/// pattern that produced `Scss,,` fails here rather than in review.
fn one(
    language: Language,
    file: &str,
    src: &str,
    pattern: &str,
    template: &str,
) -> (usize, String) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, src).expect("write");
    let scanned = scan(tmp.path(), &ScanOptions::default()).expect("scan");
    let index = Index::build_from_scan(&scanned).expect("index");

    let result = restructure::apply(&index, language, pattern, template)
        .unwrap_or_else(|e| panic!("{language} pattern '{pattern}' failed: {e}"));
    let rewritten = match result.edits.edits_for(&path) {
        Some(edits) => apply_to_string(src, edits).expect("apply"),
        None => src.to_string(),
    };
    plan(&result.edits, Validation::ReparseStrict)
        .unwrap_or_else(|e| panic!("{language} rewrite did not survive reparse: {e}"));
    (result.matches.len(), rewritten)
}

const RUST: &str = "\
pub enum Language {
    Css,
    Scss,
}

pub struct Held {
    pub run: fn() -> f64,
}

pub fn name(l: Language) -> &'static str {
    match l {
        Language::Css => \"css\",
        Language::Scss => \"scss\",
    }
}

pub fn styles(l: Language) -> bool {
    matches!(l, Language::Css | Language::Scss)
}
";

#[test]
fn a_variant_joins_an_enum() {
    let (n, out) = one(Language::Rust, "a.rs", RUST, "Scss,", "Scss,\n    Sass,");
    assert_eq!(n, 1, "the variant, and not every `Scss` in the file");
    assert!(out.contains("    Scss,\n    Sass,\n}"), "{out}");
}

#[test]
fn a_variant_pattern_is_not_the_name_it_holds() {
    // `Scss` inside `Language::Scss` is the same identifier, in a place where a variant
    // may not go. Matching it would put a declaration inside an expression.
    let (n, out) = one(Language::Rust, "a.rs", RUST, "Scss,", "Sass,");
    assert_eq!(n, 1, "{out}");
    assert!(
        out.contains("Language::Scss => \"scss\""),
        "the arm is untouched: {out}"
    );
}

#[test]
fn a_field_joins_a_struct() {
    let (n, out) = one(
        Language::Rust,
        "a.rs",
        RUST,
        "pub run: fn() -> f64,",
        "pub run: fn() -> f64,\n    pub name: &'static str,",
    );
    assert_eq!(n, 1);
    assert!(out.contains("pub name: &'static str,\n}"), "{out}");
}

#[test]
fn an_arm_joins_a_match() {
    let (n, out) = one(
        Language::Rust,
        "a.rs",
        RUST,
        "Language::Scss => \"scss\",",
        "Language::Scss => \"scss\",\n        Language::Sass => \"sass\",",
    );
    assert_eq!(n, 1);
    assert!(out.contains("Language::Sass => \"sass\","), "{out}");
}

#[test]
fn an_or_pattern_widens_wherever_it_is_written() {
    // Once in an arm, where the grammar gives it a node, and once inside `matches!`,
    // where it is a run of tokens and nothing else.
    let src = format!("{RUST}\npub fn either(l: Language) -> bool {{\n    match l {{\n        Language::Css | Language::Scss => true,\n        _ => false,\n    }}\n}}\n");
    let (n, out) = one(
        Language::Rust,
        "a.rs",
        &src,
        "Language::Css | Language::Scss",
        "Language::Css | Language::Scss | Language::Sass",
    );
    assert_eq!(n, 2, "the arm and the macro: {out}");
    assert!(
        out.contains("matches!(l, Language::Css | Language::Scss | Language::Sass)"),
        "{out}"
    );
    assert!(
        out.contains("Language::Css | Language::Scss | Language::Sass => true"),
        "{out}"
    );
}

#[test]
fn a_metavariable_inside_a_macro_binds_a_whole_argument() {
    // The argument is a call with a comma of its own. A run that stopped at the first
    // comma would bind `item.name(` and leave the rest.
    let src = "\
fn f(items: &[Item]) {
    println!(\"item {} of {}\", item.name(), items.len());
}
";
    let (n, out) = one(
        Language::Rust,
        "a.rs",
        src,
        "println!(\"item {} of {}\", $A, $B)",
        "tracing::info!(\"item {} of {}\", $B, $A)",
    );
    assert_eq!(n, 1);
    assert!(
        out.contains("tracing::info!(\"item {} of {}\", items.len(), item.name())"),
        "{out}"
    );
}

#[test]
fn a_macro_pattern_needs_every_token_of_the_run() {
    let src = "fn f() {\n    assert!(matches!(k, Kind::A | Kind::B));\n}\n";
    let (n, out) = one(
        Language::Rust,
        "a.rs",
        src,
        "Kind::A | Kind::C",
        "Kind::A | Kind::D",
    );
    assert_eq!(n, 0, "a different token in the middle is a different run");
    assert!(out.contains("Kind::A | Kind::B"), "{out}");
}

#[test]
fn a_field_joins_a_go_struct() {
    let src = "package p\n\ntype Config struct {\n\tName string\n\tPort int\n}\n";
    let (n, out) = one(
        Language::Go,
        "a.go",
        src,
        "Port int",
        "Port int\n\tHost string",
    );
    assert_eq!(n, 1);
    assert!(out.contains("\tPort int\n\tHost string\n}"), "{out}");
}

#[test]
fn a_case_joins_a_go_switch() {
    let src = "\
package p

func kind(l string) int {
	switch l {
	case \"css\":
		return 1
	}
	return 0
}
";
    let (n, out) = one(
        Language::Go,
        "a.go",
        src,
        "case \"css\":\n\t\treturn 1",
        "case \"css\":\n\t\treturn 1\n\tcase \"sass\":\n\t\treturn 3",
    );
    assert_eq!(n, 1);
    assert!(
        out.contains("\tcase \"sass\":\n\t\treturn 3\n\t}"),
        "the case keeps its own line: {out}"
    );
}

#[test]
fn a_member_joins_a_typescript_interface() {
    let src = "interface Config {\n  name: string;\n  port: number;\n}\n";
    let (n, out) = one(
        Language::TypeScript,
        "a.ts",
        src,
        "port: number;",
        "port: number;\n  host: string;",
    );
    assert_eq!(n, 1);
    assert!(out.contains("  host: string;\n}"), "{out}");
}

#[test]
fn a_property_joins_a_typescript_object() {
    let src = "const table = {\n  css: 1,\n  scss: 2,\n};\n";
    let (n, out) = one(
        Language::TypeScript,
        "a.ts",
        src,
        "scss: 2,",
        "scss: 2,\n  sass: 3,",
    );
    assert_eq!(n, 1);
    assert!(out.contains("  scss: 2,\n  sass: 3,\n}"), "{out}");
}

#[test]
fn a_constant_joins_a_java_enum() {
    let src = "enum Language {\n    CSS,\n    SCSS,\n}\n";
    let (n, out) = one(Language::Java, "A.java", src, "SCSS,", "SCSS,\n    SASS,");
    assert_eq!(n, 1);
    assert!(out.contains("    SCSS,\n    SASS,\n}"), "{out}");
}

#[test]
fn an_entry_joins_a_python_dictionary() {
    let src = "TABLE = {\n    \"css\": 1,\n    \"scss\": 2,\n}\n";
    let (n, out) = one(
        Language::Python,
        "a.py",
        src,
        "\"scss\": 2,",
        "\"scss\": 2,\n    \"sass\": 3,",
    );
    assert_eq!(n, 1);
    assert!(out.contains("    \"sass\": 3,\n}"), "{out}");
}

#[test]
fn an_attribute_joins_a_python_class() {
    let src = "class Config:\n    name = \"x\"\n    port = 8080\n";
    let (n, out) = one(
        Language::Python,
        "a.py",
        src,
        "port = 8080",
        "port = 8080\n    host = \"h\"",
    );
    assert_eq!(n, 1);
    assert!(out.contains("    port = 8080\n    host = \"h\"\n"), "{out}");
}

#[test]
fn a_field_joins_a_zig_struct() {
    let src = "const Config = struct {\n    name: []const u8,\n    port: u16,\n};\n";
    let (n, out) = one(
        Language::Zig,
        "a.zig",
        src,
        "port: u16,",
        "port: u16,\n    host: []const u8,",
    );
    assert_eq!(n, 1);
    assert!(out.contains("    host: []const u8,\n};"), "{out}");
}

#[test]
fn a_prong_joins_a_zig_switch() {
    let src = "\
pub fn kind(l: u8) u8 {
    switch (l) {
        1 => return 2,
        else => return 0,
    }
}
";
    let (n, out) = one(
        Language::Zig,
        "a.zig",
        src,
        "1 => return 2,",
        "1 => return 2,\n        3 => return 4,",
    );
    assert_eq!(n, 1);
    assert!(
        out.contains("        3 => return 4,\n        else"),
        "{out}"
    );
}
