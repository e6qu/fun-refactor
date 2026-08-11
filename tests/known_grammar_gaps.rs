//! Valid source the grammars cannot read.
//!
//! Each of these is accepted by the language's own reference implementation and produces
//! an error node here. They are recorded and not worked around, and they are pinned
//! and not merely written down, for two reasons pointing opposite ways: a grammar
//! upgrade that fixes one should be noticed and the entry retired, and a grammar that
//! starts reading one of these *without* an error node while still building the wrong
//! tree would be worse than the error, a wrong answer with nothing to say it is one.
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
    // position is read fine, so this is narrow, but `g = 1, *"ten"` appears in black's
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

/// `.sass` maps to `Language::Scss`, and the indented syntax is not SCSS.
///
/// Sass has two syntaxes: the braced one in `.scss` files, and the older
/// whitespace-significant one in `.sass` files. `tree-sitter-scss` implements the
/// first. The extension table names both, so a `.sass` file is scanned and then fails
/// to parse, visible in `fr parse`, unlike an extension that maps to nothing at all,
/// but still a claim of support the grammar cannot meet.
#[test]
fn the_indented_sass_syntax_is_not_scss() {
    assert!(error_nodes(Language::Scss, ".button\n  color: red\n") > 0);
    assert_eq!(error_nodes(Language::Scss, ".button { color: red; }\n"), 0);
}

/// The SCSS forms behind B11, and what each one costs in `twbs/bootstrap`.
///
/// Interpolation in a declaration value is not here: `Parsers::parse` masks it, which
/// is what tests/scss_interpolation.rs covers. The grammar still cannot read it. The
/// rest produce error nodes that stay inside the construct, so the file around them
/// still yields facts, masking them too was measured and recovered nothing.
#[test]
fn scss_cannot_read_these_forms() {
    let cases = [
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
        // Not in B11 until this test found it: nesting a rule under an explicit
        // combinator. 10 of the 99 files write it.
        (
            "a nested rule opening with `>`",
            ".a {\n  > .b { color: red; }\n}",
        ),
        (
            "a nested rule opening with `+`",
            ".a {\n  + .b { color: red; }\n}",
        ),
        (
            "a nested selector list opening with `>`",
            ".a {\n  > .b, > .c { color: red; }\n}",
        ),
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
/// `grafana/grafana`. It parses, bare, nested, and with arguments, so the claim was
/// either wrong when written or fixed upstream since, and nothing re-checked it. These
/// are here so that a regression is a failure and not a quietly wider limitation.
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
/// own and call it, and 177 of the 178 Go files that fail to parse in `grafana/grafana`
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
/// `const Foo = struct {};` is ordinary Zig. It is the only parse failure across 29
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

/// Helm masking: what the YAML grammar is given where an action stood.
///
/// Masking replaces `{{ … }}` with bytes of identical length so every offset in the tree
/// still indexes the original file. Which bytes matters, and a run of spaces is not
/// always legal YAML. Each case below cost files in `bitnami/charts`, where 48 of 92
/// stylesheets failed to parse before these; 4 still do, all of them the key-position
/// case the masking leaves visibly wrong on purpose.
#[test]
fn helm_masking_produces_parseable_yaml() {
    let cases = [
        // An action supplying the block indented under it: the value must end up empty,
        // not a scalar, or the deeper mapping has nothing to attach to.
        (
            "an action supplying a block",
            "metadata:\n  labels: {{- include \"common.labels.standard\" . | nindent 4 }}\n    app: node\n",
        ),
        // The first line of a block scalar: YAML rejects a leading empty line indented
        // further than the content, and a masked action is as wide as the action was.
        (
            "an action on a block scalar's first line",
            "data:\n  redis.conf: |-\n    {{- $password := include \"redis.password\" . }}\n    user default on nopass\n",
        ),
        // The same, for a block scalar opened by a sequence item.
        (
            "a block scalar opened by a sequence item",
            "args:\n  - |\n    {{- if .Values.enabled }}\n    echo hello\n",
        ),
        // An action running over a newline: only the line it starts on takes the scalar
        // filler, or the continuation lands at column zero and ends the block.
        (
            "an action spanning two lines",
            "data:\n  script: |-\n    NAMES=\"{{\n    $fullname := include \"common.names.fullname\" . -}}\"\n",
        ),
        // A template comment is opaque, `{{` and all.
        (
            "a template comment containing an action",
            "data:\n  c: |-\n    {{- /* #j={{ $j }} */}}\n    text\n",
        ),
    ];
    for (what, source) in cases {
        assert_eq!(
            error_nodes(Language::Helm, source),
            0,
            "{what} should parse:\n{source}"
        );
    }
}

/// And the one the masking leaves wrong on purpose.
///
/// A key supplied by a template is reported and not invented, because a
/// plausible-looking fake key hides more than a parse error does.
#[test]
fn helm_leaves_a_templated_key_visibly_wrong() {
    assert!(error_nodes(Language::Helm, "params:\n  {{ $key }}:\n    - a\n") > 0);
}
