//! The everyday Zig forms read, and a failed initializer keeps its binding.
//!
//! `[_]u32{ 1, 2, 3 }`, `items[i]`, `@intCast(u8, b)`, `&a`, `p.*` and the
//! dot-literal `.empty` are the forms an ordinary Zig file is made of. Every
//! one carried whole, taking its declaration with it. The statements after each
//! then read names the output never declared.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const FORMS_ZIG: &str = "// A probe file.\n const std = @import(\"std\");\n\n\
    pub fn one() u32 {\n    const a: u32 = 1;\n    var list: std.ArrayList(u32) = .empty;\n    \
    const n = @intCast(u8, a);\n    const p = &a;\n    const q = p.*;\n    \
    const items = [_]u32{ 1, 2, 3 };\n    const first = items[0];\n    \
    const src = @src();\n    return a + n + q + first;\n}\n";

fn to_typescript(source: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("forms.zig");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("forms_out.txt");
    transpile::plan_to(&path, Language::TypeScript, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn the_everyday_forms_cross() {
    let out = to_typescript(FORMS_ZIG);
    for expected in [
        "const items = [1, 2, 3];",
        "const first = items[0];",
        "const n = (a as u8);",
        "const p = a;",
        "const q = p;",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }
}

#[test]
fn a_dot_literal_qualifies_with_the_declared_type() {
    let out = to_typescript(FORMS_ZIG);
    assert!(
        out.contains(".empty;")
            && !out.contains("not translated: variable_declaration from line 5"),
        "`.empty` reads as a member of the annotation's type.\n{out}"
    );
}

#[test]
fn a_failed_initializer_keeps_its_binding() {
    let out = to_typescript(FORMS_ZIG);
    assert!(
        out.contains("const src: any = null /* fun-refactor: not translated: @src() */;"),
        "the name stays declared as `any`, so strict TypeScript accepts it.\n{out}"
    );
}

#[test]
fn a_carried_comment_never_holds_a_tab_zig_refuses() {
    // Zig rejects a tab inside a comment, and carried source brings the
    // indentation the other language wrote. A Go file's tabs produced a Zig
    // file its own compiler would not lex.
    let source = "package tabs\n\nimport (\n\t\"fmt\"\n\t\"strings\"\n)\n\n\
        func Shout(name string) string {\n\t\
        return strings.ToUpper(fmt.Sprintf(\"%v!\", name))\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tabs.go");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("tabs_out.txt");
    let written = transpile::plan_to(&path, Language::Zig, Some(&out), false)
        .expect("a plan")
        .output;
    assert!(
        !written.contains('\t'),
        "no tab reaches a Zig comment.\n{written}"
    );
}
