//! Source the published grammars read wrongly, and what this build does with it.

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
fn python_reads_a_starred_element_in_a_bare_tuple() {
    // `grammars/python` gives `expression_list` the choice Python's own star_expressions
    // has: each element is an expression or a starred one.
    for source in [
        "g = 1, *[2]\n",
        "g = 1, *(2,)\n",
        "g = 1, *{2}\n",
        "g = 1, *\"ab\"\n",
        "g = *\"ab\", 1\n",
        "def f():\n    return 1, *[2]\n",
    ] {
        assert_eq!(
            error_nodes(Language::Python, source),
            0,
            "`{}` is ordinary Python",
            source.trim()
        );
    }
}

#[test]
fn python_reads_the_forms_around_that_one() {
    // The boundary of the starred-element rule, so a fix that over-corrects shows up.
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
fn python_reads_a_type_parameter_default() {
    // PEP 696, Python 3.13.
    for source in [
        "type A[T = int] = float\n",
        "type A[T] = float\n",
        "type P[T: int = bool] = float\n",
    ] {
        assert_eq!(
            error_nodes(Language::Python, source),
            0,
            "`{}` is ordinary Python",
            source.trim()
        );
    }
}

/// An import type stands where any other type may.
#[test]
fn typescript_reads_an_import_type() {
    for source in [
        "type A = { ast?: import(\"@babel/types\").Statement[] }\n",
        "type A = import(\"@babel/types\").Statement[]\n",
        "type A = import(\"m\").S<number>\n",
        "type A = import(\"m\").S.T[]\n",
        "let x: import(\"m\").S.T\n",
        "type A = import(\"m\").S | null\n",
        "type A = typeof import(\"m\").x\n",
    ] {
        assert_eq!(
            error_nodes(Language::TypeScript, source),
            0,
            "`{}` is ordinary TypeScript",
            source.trim()
        );
    }
}

/// A member called `in` reads as a member, wherever it sits in the body.
#[test]
fn typescript_reads_a_property_called_in() {
    for source in [
        "interface G {\n  a?: string\n  in?: string\n}\n",
        "interface G {\n  in?: string\n}\n",
        "interface G {\n  a?: string\n  in: string\n}\n",
        "interface G {\n  a?: string\n  instanceof?: string\n}\n",
        "interface G {\n  result?: string\n  in?: string\n  in2?: string\n}\n",
    ] {
        assert_eq!(
            error_nodes(Language::TypeScript, source),
            0,
            "a member may be called `in`: {source}"
        );
    }
}

/// And the operator either side of it, which the same rule decides.
#[test]
fn typescript_reads_the_in_operator_across_a_line_break() {
    for source in [
        // The line break before `in` does not end the expression.
        "function f(a: string, b: object) {\n  const y = a\n  in b ? 1 : 2\n}\n",
        "function f(a: string, b: object) {\n  const y = a\n  instanceof Object ? 1 : 2\n}\n",
        // An identifier that only begins with `in` is a new statement.
        "function f() {\n  const x = 1\n  in2(x)\n}\n",
        "function f() {\n  const x = 1\n  instanceofThing(x)\n}\n",
        // A mapped type writes `in` as an operator of its own, inside brackets.
        "type M<T> = { [K in keyof T]: string }\n",
        "type M<T> = { [K\n  in keyof T]: string }\n",
    ] {
        assert_eq!(
            error_nodes(Language::TypeScript, source),
            0,
            "`in` still reads as an operator where one can stand: {source}"
        );
    }
}

/// The indented Sass syntax, which is a language of its own and has a grammar of its own.
#[test]
fn sass_reads_the_indented_syntax() {
    let cases = [
        ("a rule", ".button\n  color: red\n"),
        ("a nested rule", ".a\n  .b\n    color: red\n"),
        ("a parent selector", ".a\n  &:hover\n    color: blue\n"),
        ("a variable", "$primary: #3498db\n"),
        ("`!default`", "$x: 1rem !default\n"),
        ("an interpolated selector", ".a-#{$x}\n  color: red\n"),
        ("an interpolated value", ".a\n  color: #{$v}\n"),
        ("a mixin", "@mixin m($a, $b: 2)\n  margin: $a\n"),
        ("a variadic mixin", "@mixin m($shadow...)\n  color: red\n"),
        ("an include", ".a\n  @include m(1)\n"),
        (
            "a namespaced include",
            "@use \"x\" as t\n\n.a\n  @include t.m(2)\n",
        ),
        ("a function", "@function f($x)\n  @return $x * 2\n"),
        (
            "an `@if` with `and`",
            "@if $a == 1 and $b == 2\n  .a\n    color: red\n",
        ),
        (
            "an `@each`",
            "@each $k, $v in $m\n  .e-#{$k}\n    width: $v\n",
        ),
        (
            "a `@for`",
            "@for $i from 1 through 3\n  .c-#{$i}\n    width: $i\n",
        ),
        ("`@use ... with`", "@use \"x\" with ($a: 1)\n"),
        ("`@forward ... as`", "@forward \"x\" as t-*\n"),
        ("a placeholder", "%p\n  color: red\n\n.a\n  @extend %p\n"),
        (
            "a media query",
            "@media (min-width: 1px)\n  .a\n    color: red\n",
        ),
        (
            "a keyframes block",
            "@keyframes slide\n  0%\n    opacity: 0\n",
        ),
        ("a custom property", ".a\n  --gap: 4px\n"),
        ("`var()`", ".a\n  margin: var(--gap)\n"),
        ("a negated variable", ".a\n  margin: -$x\n"),
        // The six the patch is for.
        (
            "a colour word in a value",
            ".a\n  transition: color 0.2s ease\n",
        ),
        (
            "a namespaced call",
            ".a\n  color: color.adjust($c, $lightness: -10%)\n",
        ),
        (
            "a named argument",
            ".a\n  color: adjust($c, $lightness: 1)\n",
        ),
        ("a list in parentheses", "$d: (none, inline, block)\n"),
        (
            "an `@each` over a list",
            "@each $s in (0, 1, 2)\n  .c-#{$s}\n    width: $s\n",
        ),
        ("a selector list over two lines", ".a,\n.b\n  color: red\n"),
        ("two interpolations joined", ".#{$a}-#{$b}\n  color: red\n"),
        ("a combinator with spaces", "li + li\n  color: red\n"),
        ("a nested combinator", ".a\n  & + li\n    color: red\n"),
    ];
    for (what, source) in cases {
        assert_eq!(
            error_nodes(Language::Sass, source),
            0,
            "{what} is ordinary Sass: {source}"
        );
    }
}

/// And the braced syntax stays SCSS's, which is a different grammar reading a different
/// file extension.
#[test]
fn the_two_sass_syntaxes_keep_their_own_grammars() {
    assert_eq!(error_nodes(Language::Scss, ".button { color: red; }\n"), 0);
    assert_eq!(error_nodes(Language::Sass, ".button\n  color: red\n"), 0);
    // Neither reads the other: the braced syntax has no meaning in a `.sass` file, and
    // the indented one has none in a `.scss` file.
    assert!(error_nodes(Language::Scss, ".button\n  color: red\n") > 0);
    assert!(error_nodes(Language::Sass, ".button { color: red; }\n") > 0);
    assert_eq!(
        fun_refactor::lang::detect(std::path::Path::new("style.sass")),
        Some(Language::Sass)
    );
    assert_eq!(
        fun_refactor::lang::detect(std::path::Path::new("style.scss")),
        Some(Language::Scss)
    );
}

/// The Sass the published grammar could not read.
#[test]
fn scss_reads_these_forms() {
    let cases = [
        // A declaration whose value holds an interpolation.
        ("interpolation in a value", ".a { color: #{$v}; }"),
        ("interpolation in a custom property", ".a { --x: #{$v}; }"),
        ("interpolation among values", ".a { width: 1px #{$v}; }"),
        ("interpolation joined to a value", ".a { color: a#{$v}; }"),
        (
            "several values in an interpolation",
            ".a { --x: #{transform 1s ease-in-out}; }",
        ),
        // Argument lists, in every shape one takes.
        ("empty parameters", "@mixin m() { color: red; }"),
        (
            "empty arguments",
            "@mixin m { color: red; }\n.a { @include m(); }",
        ),
        (
            "variadic parameters",
            "@mixin m($shadow...) { color: red; }",
        ),
        (
            "a variadic argument",
            "@mixin m($a...) { color: red; }\n.a { @include m($params...); }",
        ),
        (
            "a variadic call argument",
            "$m: map-merge($m, ($key: call(get-function($f), $args...)));",
        ),
        (
            "a parameter default of several values",
            "@mixin m($a, $b: 0 0 $x rgba($c, .5)) { color: red; }",
        ),
        (
            "named arguments over lines",
            ".a {\n  @include m(\n    $v,\n    $hover: shade($v, 1),\n  );\n}",
        ),
        (
            "a space before the arguments",
            ".a { @include m () { color: red; } }",
        ),
        // Conditions.
        (
            "`and` in an `@if`",
            "@if $a == 1 and $b == 2 { .a { color: red; } }",
        ),
        (
            "`or` in an `@if`",
            "@if $a == 1 or $b == 2 { .a { color: red; } }",
        ),
        ("`not` in an `@if`", "@if not $a { .a { color: red; } }"),
        ("`%` as the modulo operator", "$x: $l % 10;"),
        ("a map literal", "$m: (a: 1, b: 2);"),
        ("a nested map literal", "$m: (a: (b: 1));"),
        ("a map with a trailing comma", "$m: (a: 1, b: 2,);"),
        ("a map without a space after the colon", "$m: (a:1, b:2);"),
        (
            "a map key built from an expression",
            "$m: map-merge($m, (\"n\" + $key: (-$value)));",
        ),
        (
            "several values in a map entry",
            "$m: (\"a\": 0 0 $x rgba(1, 2, 3, .5), \"b\": 1);",
        ),
        ("an empty list", "$m: ();"),
        ("a one-element list", "$v: if($x != list, ($v,), $v);"),
        (
            "a list of runs",
            "$m: (\"s\": (0 1px 2px red, 0 0 0 1px blue));",
        ),
        // Flags.
        ("`!default`", "$x: 1rem !default;"),
        ("`!global`", ".a { $x: 1 !global; }"),
        (
            "`!default` as the last declaration",
            ".a { $x: 1rem !default }",
        ),
        // The module system.
        ("`@use ... as`", "@use \"x\" as t;"),
        ("`@use ... as *`", "@use \"x\" as *;"),
        ("`@use ... with`", "@use \"x\" with ($a: 1);"),
        ("`@forward ... as`", "@forward \"x\" as t-*;"),
        ("`@forward ... show`", "@forward \"x\" show $a, b;"),
        ("`@forward ... hide`", "@forward \"x\" hide $a, b;"),
        (
            "`@forward ... with`",
            "@forward \"x\" with ($a: 1 !default);",
        ),
        (
            "a namespaced `@include`",
            "@use \"x\" as t;\n.a { @include t.m(1); }",
        ),
        // Selectors.
        (
            "a nested rule opening with `>`",
            ".a {\n  > .b { color: red; }\n}",
        ),
        (
            "a nested rule opening with `+`",
            ".a {\n  + .b { color: red; }\n}",
        ),
        (
            "a nested rule opening with `~`",
            ".a {\n  ~ .b { color: red; }\n}",
        ),
        (
            "a nested selector list opening with `>`",
            ".a {\n  > .b, > .c { color: red; }\n}",
        ),
        (
            "a mixed nested selector list",
            ".a {\n  > .b,\n  .c { color: red; }\n}",
        ),
        (
            "a relative selector in `:has`",
            ".a:has(+ .b) { color: red; }",
        ),
        ("a step of `n + 3`", ".a:nth-child(n + 3) { color: red; }"),
        (
            "a step of `-n + 2`",
            ".a:nth-last-child(-n + 2) { color: red; }",
        ),
        ("an interpolated placeholder", "%p-#{$b} { color: red; }"),
        (
            "extending a placeholder",
            "%p { color: red; }\n.a { @extend %p; }",
        ),
        (
            "extending an interpolated placeholder",
            ".a { @extend %p-#{$b}; }",
        ),
        (
            "interpolation after `::`",
            ".a { &::#{$el} { color: red; } }",
        ),
        (
            "interpolation after a pseudo class",
            ".a { > :not(:first-child)#{$m} { color: red; } }",
        ),
        (
            "an interpolated class with a number",
            ".#{$p}-500px { color: red; }",
        ),
        (
            "two interpolated classes",
            ".#{$p}.#{$p}-500px { color: red; }",
        ),
        (
            "an interpolated attribute name",
            "[data-#{$p}theme=\"#{$n}\"] { color: red; }",
        ),
        ("an escape in a selector", ".a\\.5 { color: red; }"),
        // Values and at-rules.
        ("an escape in a value", "$a: \\f26e;"),
        (
            "a negated value in parentheses",
            ".a { margin: (-$x) (-$y); }",
        ),
        (
            "an unquoted data url",
            ".a { background-image: url(data:image/svg+xml;charset=utf-8,%3Csvg%2F%3E); }",
        ),
        (
            "`@container`",
            "@container c (min-width: 1px) { .a { color: red; } }",
        ),
        (
            "an interpolated `@container`",
            "@container #{$n} (min-width: #{$w}) { .a { color: red; } }",
        ),
        (
            "a fractional keyframe step",
            "@keyframes a { 3.4% { opacity: 0; } }",
        ),
        (
            "a keyframe step list",
            "@keyframes a { 0%, 90% { opacity: 0; } }",
        ),
        (
            "an interpolated keyframes name",
            "@keyframes #{$p}-beat { 0% { opacity: 0; } }",
        ),
        (
            "a list in `@return`",
            "@function f($v) { @return red($v), green($v); }",
        ),
        (
            "an interpolated feature query",
            "@media (#{$a}: #{$b}) { .a { color: red; } }",
        ),
    ];
    for (what, source) in cases {
        assert_eq!(
            error_nodes(Language::Scss, source),
            0,
            "{what} is ordinary Sass: {source}"
        );
    }
}

/// And the forms it can, which B11 claimed one of.
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

/// A call to a user-defined `new` or `make` parses.
#[test]
fn go_reads_a_call_to_a_user_defined_new() {
    for source in [
        "package main\n\nfunc new(s string) string { return s }\n\n\
         func use() string {\n\treturn new(\"-10s\")\n}\n",
        "package main\n\nfunc use(err error) string {\n\treturn new(err.Error())\n}\n",
        "package main\n\nfunc use(n int) []int {\n\treturn make(sizeFor(n))\n}\n",
    ] {
        assert_eq!(
            error_nodes(Language::Go, source),
            0,
            "a package may define `new` and call it: {source}"
        );
    }
}

/// And the builtin keeps the tree it had, which takes a type where an expression goes.
#[test]
fn go_reads_the_builtin_forms_of_new_and_make() {
    for source in [
        "package main\n\nfunc f() {\n\t_ = new(int)\n\t_ = new([]byte)\n}\n",
        "package main\n\nfunc f() {\n\t_ = make(map[string]int, 4)\n\t_ = make(chan int)\n}\n",
        "package main\n\nfunc f() {\n\t_ = make([]int, 0, 10)\n\t_ = new(struct{ x int })\n}\n",
    ] {
        assert_eq!(
            error_nodes(Language::Go, source),
            0,
            "the builtin still reads a type: {source}"
        );
    }
    let parsed = Parsers::new()
        .parse(
            Language::Go,
            "package main\n\nfunc f() {\n\t_ = make(map[string]int, 4)\n}\n",
        )
        .expect("the grammar loads");
    assert!(
        parsed.tree.root_node().to_sexp().contains("map_type"),
        "the reader still takes the first argument for a type"
    );
}

/// A container with no members holds no member, in all four spellings Zig gives one.
#[test]
fn zig_reads_a_container_with_no_members() {
    let parsers = Parsers::new();
    for source in [
        "const Foo = struct {};\n",
        "const E = enum {};\n",
        "const U = union {};\n",
        "const O = opaque {};\n",
        "const P = packed struct {};\n",
    ] {
        assert_eq!(error_nodes(Language::Zig, source), 0, "{source}");
        let tree = parsers
            .parse(Language::Zig, source)
            .expect("the grammar loads")
            .tree
            .root_node()
            .to_sexp();
        assert!(
            !tree.contains("container_field"),
            "an empty container declares no field: {source} gave {tree}"
        );
    }
    let with_member = parsers
        .parse(Language::Zig, "const Bar = struct { x: i32 };\n")
        .expect("the grammar loads")
        .tree
        .root_node()
        .to_sexp();
    assert!(
        with_member.contains("container_field"),
        "and one with a member still declares it: {with_member}"
    );
}

/// Helm masking: what the YAML grammar sees where an action stood.
#[test]
fn helm_masking_produces_parseable_yaml() {
    let cases = [
        // An action supplying the block indented under it.
        (
            "an action supplying a block",
            "metadata:\n  labels: {{- include \"common.labels.standard\" . | nindent 4 }}\n    app: node\n",
        ),
        // The first line of a block scalar: YAML rejects a leading empty line indented further
        // than the content.
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
#[test]
fn helm_leaves_a_templated_key_visibly_wrong() {
    assert!(error_nodes(Language::Helm, "params:\n  {{ $key }}:\n    - a\n") > 0);
}

/// `mutual` is the only form Lean has for a cycle, and the published grammar has no rule
/// for it at all. This build's Lean writer emits one wherever the order it computes finds
/// a cycle, so the rule is in `grammars/lean`.
#[test]
fn lean_reads_a_mutual_block() {
    for source in [
        "mutual\npartial def a (n : Int) : Int := b n\npartial def b (n : Int) : Int := a n\nend\n",
        "mutual\ndef a : Int := 1\ndef b : Int := 2\nend\n",
        "mutual\nstructure A where\n  x : Int\nstructure B where\n  y : Int\nend\n",
    ] {
        assert_eq!(
            error_nodes(Language::Lean, source),
            0,
            "`mutual` is ordinary Lean:\n{source}"
        );
    }
}

/// The boundary of that rule, so a fix that over-corrects shows up.
#[test]
fn lean_reads_the_forms_around_a_mutual_block() {
    for source in [
        "def a : Int := 1\n",
        "namespace N\ndef a : Int := 1\nend N\n",
        "section\ndef a : Int := 1\nend\n",
    ] {
        assert_eq!(
            error_nodes(Language::Lean, source),
            0,
            "`{}` should read cleanly",
            source.trim()
        );
    }
}

/// A `while` whose condition is a comparison needs brackets around it. Lean applies a
/// function by writing its argument beside it, and `3 do ...` is an application. Lean
/// itself forbids that reading. The grammar cannot, so the writer brackets instead.
#[test]
fn lean_reads_a_bracketed_while_condition() {
    let bracketed =
        "def g : IO Unit := do\n  let mut i : Int := 0\n  while (i < 3) do\n    i := i + 1\n";
    assert_eq!(error_nodes(Language::Lean, bracketed), 0, "{bracketed}");
    let bare = "def g : IO Unit := do\n  let mut i : Int := 0\n  while i < 3 do\n    i := i + 1\n";
    assert!(
        error_nodes(Language::Lean, bare) > 0,
        "the bare form is the one the writer avoids; if it reads cleanly now, the \
         brackets in `write/lean.rs` have stopped earning their place"
    );
}

/// A trailing `else` that dedents past the `if` it belongs to. B832.
///
/// The boundary is the column, not the `else if`: written under the inner `if` the
/// whole chain reads, and nobody writes it that way.
#[test]
fn lean_leaves_a_dedented_trailing_else_visibly_wrong() {
    let arms = |body: &str| {
        format!("def f (d : Int) : String := Id.run do\n  if d == 1 then\n    return \"a\"\n  else if d == 2 then\n{body}")
    };
    // Both `else` branches attach when the last one sits under the inner `if`.
    let indented = arms("    return \"b\"\n       else\n         return \"c\"\n");
    assert_eq!(error_nodes(Language::Lean, &indented), 0, "{indented}");
    let tree = Parsers::new()
        .parse(Language::Lean, &indented)
        .expect("the grammar loads");
    assert_eq!(
        branches(tree.root()),
        2,
        "both arms belong to an `if`:\n{indented}"
    );

    // Dedented to the outer column, the second one does not.
    let dedented = arms("    return \"b\"\n  else\n    return \"c\"\n");
    assert_eq!(
        error_nodes(Language::Lean, &dedented),
        0,
        "B832 is a wrong tree and not an error node"
    );
    let tree = Parsers::new()
        .parse(Language::Lean, &dedented)
        .expect("the grammar loads");
    assert_eq!(
        branches(tree.root()),
        1,
        "B832 says the second `else` is lost. If both attach now, close the entry."
    );
}

/// How many `if` nodes in this tree carry an `else`.
fn branches(node: tree_sitter::Node<'_>) -> usize {
    let mut found = node.child_by_field_name("else").is_some() as usize;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        found += branches(child);
    }
    found
}

/// Two chained `let`s inside a branch, which the published grammar cannot read. Nothing
/// this build writes takes the form: the Lean writer's bodies are `do` blocks.
#[test]
fn lean_leaves_a_chained_let_in_a_branch_visibly_wrong() {
    let chained =
        "def f (n : Int) : Int :=\n  if n < 0 then n else\n    let a := n\n    let b := a\n    b\n";
    assert!(
        error_nodes(Language::Lean, chained) > 0,
        "B824 names this shape. If it reads cleanly now, close the entry."
    );
    // The forms around it, which do read, and which the writer uses instead.
    for source in [
        "def f (n : Int) : Int :=\n  if n < 0 then n else\n    let a := n\n    a\n",
        "def f (n : Int) : Int :=\n  let a := n\n  let b := a\n  b\n",
        "def f (n : Int) : Int := Id.run do\n  let a := n\n  let b := a\n  return b\n",
    ] {
        assert_eq!(
            error_nodes(Language::Lean, source),
            0,
            "the boundary of B824 should read cleanly.\n{source}"
        );
    }
}

/// A comment as the last line of a `do` block leaves the layout open, and the
/// declaration after it lands inside the block. Lean reads a comment as whitespace
/// anywhere; the scanner here ends a block on indentation and a comment carries none.
#[test]
fn lean_leaves_a_block_ending_in_a_comment_visibly_wrong() {
    let trailing = "def a : Unit := Id.run do\n  let mut s := 0\n  s := s + 1\n  -- a trailing comment\n\ndef b : Int := 1\n";
    assert!(
        error_nodes(Language::Lean, trailing) > 0,
        "B828 names this shape. If it reads cleanly now, close the entry."
    );
    // The same block with an element after the comment, which the writer emits.
    for source in [
        "def a : Unit := Id.run do\n  let mut s := 0\n  s := s + 1\n  -- a comment\n  ()\n\ndef b : Int := 1\n",
        "def a : Unit := Id.run do\n  -- only a comment\n  ()\n\ndef b : Int := 1\n",
        "def a : Unit := Id.run do\n  let mut s := 0\n  s := s + 1\n\ndef b : Int := 1\n",
    ] {
        assert_eq!(
            error_nodes(Language::Lean, source),
            0,
            "the boundary of B828 should read cleanly.\n{source}"
        );
    }
}
