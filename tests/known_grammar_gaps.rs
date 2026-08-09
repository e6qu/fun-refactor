//! Valid source the grammars cannot read.
//!
//! Each of these is accepted by the language's own reference implementation and produces
//! an error node here. They are recorded rather than worked around, and they are pinned
//! rather than merely written down, for two reasons pointing opposite ways: a grammar
//! upgrade that fixes one should be noticed and the entry retired, and a grammar that
//! starts reading one of these *without* an error node while still building the wrong
//! tree would be worse than the error — a wrong answer with nothing to say it is one.
//!
//! Every case here has a BUGS.md entry. When a test fails, the entry is what to update.

use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;

fn error_nodes(language: Language, source: &str) -> usize {
    Parsers::new()
        .parse(language, source)
        .expect("the grammar loads")
        .error_spans()
        .len()
}

#[test]
fn python_cannot_read_a_starred_literal_in_a_bare_tuple() {
    // B233. `g = 1, *[2]` is ordinary Python. A starred *name* or *call* in the same
    // position is read fine, so this is narrow — but `g = 1, *"ten"` appears in black's
    // own test data, which is where it was found.
    for source in [
        "g = 1, *[2]\n",
        "g = 1, *(2,)\n",
        "g = 1, *{2}\n",
        "g = 1, *\"ab\"\n",
        "g = *\"ab\", 1\n",
    ] {
        assert!(
            error_nodes(Language::Python, source) > 0,
            "the grammar now reads `{}` — retire B233 if the tree is right",
            source.trim()
        );
    }
}

#[test]
fn python_reads_the_forms_around_that_one() {
    // The boundary of B233, so a fix that over-corrects is visible too.
    for source in [
        "g = 1, *rest\n",
        "g = 1, *f()\n",
        "g = (1, *[2])\n",
        "g = [1, *[2]]\n",
        "a, *b = [1, 2, 3]\n",
        "f(1, *rest)\n",
    ] {
        assert_eq!(
            error_nodes(Language::Python, source),
            0,
            "`{}` should read cleanly",
            source.trim()
        );
    }
}

#[test]
fn python_cannot_read_a_type_parameter_default() {
    // B234. PEP 696, Python 3.13.
    assert!(error_nodes(Language::Python, "type A[T = int] = float\n") > 0);
    assert_eq!(
        error_nodes(Language::Python, "type A[T] = float\n"),
        0,
        "a type alias without a default should read cleanly"
    );
}

#[test]
fn typescript_cannot_read_an_import_type() {
    // B231.
    assert!(
        error_nodes(
            Language::TypeScript,
            "type A = { ast?: import(\"@babel/types\").Statement[] }\n"
        ) > 0
    );
}

#[test]
fn typescript_cannot_read_a_property_called_in_after_another() {
    // B232. Alone it is fine, which is what makes it worth pinning from both sides.
    assert!(
        error_nodes(
            Language::TypeScript,
            "interface G {\n  a?: string\n  in?: string\n}\n"
        ) > 0
    );
    assert_eq!(
        error_nodes(Language::TypeScript, "interface G {\n  in?: string\n}\n"),
        0,
        "`in` as the only member should read cleanly"
    );
}

/// The SCSS forms behind B11, in the order they cost files in `twbs/bootstrap`.
///
/// 73 of its 99 stylesheets fail, and one form accounts for 51 of them.
#[test]
fn scss_cannot_read_these_forms() {
    let cases = [
        (
            "interpolation in a declaration value",
            ".a { color: #{$v}; }",
        ),
        (
            "interpolation in a custom property value",
            ".a { --x: #{$v}; }",
        ),
        (
            "empty parentheses on a declaration",
            "@mixin m() { color: red; }",
        ),
        (
            "empty parentheses on a call",
            "@mixin m { color: red; }\n.a { @include m(); }",
        ),
        (
            "`and` in an `@if`",
            "@if $a == 1 and $b == 2 { .a { color: red; } }",
        ),
        ("a map literal", "$m: (a: 1, b: 2);"),
        ("a nested map literal", "$m: (a: (b: 1));"),
        ("`!default`", "$x: 1rem !default;"),
        ("`@use ... as`", "@use \"x\" as t;"),
    ];
    for (what, source) in cases {
        assert!(
            error_nodes(Language::Scss, source) > 0,
            "{what} now parses — retire it from B11: {source}"
        );
    }
}

/// And the forms it can, which B11 claimed one of.
///
/// The entry said `@content` inside a mixin was among the gaps, from a run over
/// `grafana/grafana`. It parses — bare, nested, and with arguments — so the claim was
/// either wrong when written or fixed upstream since, and nothing re-checked it. These
/// are here so that a regression is a failure rather than a quietly wider limitation.
#[test]
fn scss_can_read_these_forms() {
    let cases = [
        ("`@content` bare", "@mixin m { @content; }"),
        ("`@content` nested", "@mixin m { .a { @content; } }"),
        (
            "`@content` with arguments",
            "@mixin m($x) { @content($x); }",
        ),
        ("interpolation in a selector", ".a-#{$x} { color: red; }"),
        ("interpolation in a property name", ".a { --#{$p}x: 1px; }"),
        (
            "`@each` over two variables",
            "@each $k, $v in $m { .a { color: $k; } }",
        ),
        (
            "a bare comparison in an `@if`",
            "@if $a == 1 { .a { color: red; } }",
        ),
        ("`@function`", "@function f($x) { @return $x; }"),
        ("`!important`", ".a { color: red !important; }"),
    ];
    for (what, source) in cases {
        assert_eq!(
            error_nodes(Language::Scss, source),
            0,
            "{what} stopped parsing: {source}"
        );
    }
}

/// B15: `tree-sitter-go` reads `new(…)` as the builtin, which takes a type.
///
/// `new` is a predeclared identifier in Go, not a keyword, so a package may define its
/// own and call it — and 177 of the 178 Go files that fail to parse in `grafana/grafana`
/// do exactly that.
#[test]
fn go_cannot_read_a_call_to_a_user_defined_new() {
    let source = "package main\n\nfunc new(s string) string { return s }\n\n\
                  func use() string {\n\treturn new(\"-10s\")\n}\n";
    assert!(
        error_nodes(Language::Go, source) > 0,
        "a user-defined `new` now parses — retire B15"
    );
    // The shape either side of it: a call to anything else, and the builtin's own form.
    assert_eq!(
        error_nodes(
            Language::Go,
            "package main\n\nfunc old(s string) string { return s }\n\n\
             func use() string {\n\treturn old(\"-10s\")\n}\n"
        ),
        0
    );
}

/// B133: `tree-sitter-zig` requires at least one member in a struct.
///
/// `const Foo = struct {};` is ordinary Zig — it is the only parse failure across 29
/// files of Zig's own standard library.
#[test]
fn zig_cannot_read_a_struct_with_no_members() {
    assert!(
        error_nodes(Language::Zig, "const Foo = struct {};\n") > 0,
        "an empty struct now parses — retire B133"
    );
    assert_eq!(
        error_nodes(Language::Zig, "const Bar = struct { x: i32 };\n"),
        0,
        "a struct with a member still parses"
    );
}
