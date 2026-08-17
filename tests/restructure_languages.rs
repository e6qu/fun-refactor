//! Pattern restructuring across every language in the matrix.
//!
//! One realistic pattern per language, plus the two things that make structural matching worth
//! having over `sed`. A similar-but-different shape must not match, and text inside a string or
//! a comment is not code. Every rewrite is put through the same `ReparseStrict` validation the
//! CLI uses, so a test can only pass if the rewritten file still parses.

use fun_refactor::edit::{apply_to_string, plan, Validation};
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::refactor::restructure;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;

/// A workspace holding one file per entry, indexed and ready to restructure.
struct Workspace {
    _tmp: tempfile::TempDir,
    index: Index,
    root: PathBuf,
}

fn workspace(files: &[(&str, &str)]) -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    let root = tmp.path().to_path_buf();
    Workspace {
        _tmp: tmp,
        index,
        root,
    }
}

/// Rewrite `file` and return (matches in that file, rewritten text).
///
/// Matches are counted per file because a workspace can hold more than one file of
/// the language under test, a Helm chart always carries its `Chart.yaml`.
///
/// The edit set is also run through `ReparseStrict`, so any pattern that produced a
/// syntactically broken file fails here and not in review.
fn restructured(
    files: &[(&str, &str)],
    language: Language,
    file: &str,
    pattern: &str,
    template: &str,
) -> (usize, String) {
    let ws = workspace(files);
    let result = restructure::apply(&ws.index, language, pattern, template)
        .unwrap_or_else(|e| panic!("{language} pattern '{pattern}' failed: {e}"));

    let path = ws.root.join(file);
    let original = std::fs::read_to_string(&path).unwrap();
    let rewritten = match result.edits.edits_for(&path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    };

    // The whole point of the reparse gate: a rewrite that breaks the file is not a
    // rewrite. `plan` returns an error, so `unwrap` is the assertion.
    let outcomes = plan(&result.edits, Validation::ReparseStrict)
        .unwrap_or_else(|e| panic!("{language} rewrite did not survive reparse: {e}"));
    if !result.edits.is_empty() {
        assert!(
            outcomes.iter().any(|o| o.changed()),
            "{language}: edits were planned but nothing changed"
        );
    }

    let here = result.matches.iter().filter(|(p, _)| *p == path).count();
    (here, rewritten)
}

/// Rewrite a single-file workspace.
fn one(
    language: Language,
    file: &str,
    src: &str,
    pattern: &str,
    template: &str,
) -> (usize, String) {
    restructured(&[(file, src)], language, file, pattern, template)
}

// ------------------------------------------------------------------------- rust

const RUST_SRC: &str = "\
fn f() {
    // old_api(9) is not code
    let banner = \"old_api(9)\";
    let a = old_api(1);
    let b = old_api(x);
    let c = old_api(1, 2);
    let d = other_api(1);
}
";

#[test]
fn rust_rewrites_a_call_shape() {
    let (n, out) = one(
        Language::Rust,
        "a.rs",
        RUST_SRC,
        "old_api($X)",
        "new_api($X, None)",
    );
    assert_eq!(n, 2);
    assert_eq!(
        out,
        "\
fn f() {
    // old_api(9) is not code
    let banner = \"old_api(9)\";
    let a = new_api(1, None);
    let b = new_api(x, None);
    let c = old_api(1, 2);
    let d = other_api(1);
}
"
    );
}

#[test]
fn rust_does_not_match_a_different_arity_or_name() {
    let (n, _) = one(Language::Rust, "a.rs", RUST_SRC, "old_api($X, $Y)", "z($X)");
    assert_eq!(n, 1, "only the two-argument call has that shape");
}

// --------------------------------------------------------------------------- go

const GO_SRC: &str = "\
package main

// oldAPI(9) is not code
func f() {
\tbanner := \"oldAPI(9)\"
\ta := oldAPI(1)
\tb := oldAPI(2, 3)
\tc := newAPI(1)
\t_, _, _, _ = banner, a, b, c
}
";

#[test]
fn go_rewrites_a_call_shape() {
    let (n, out) = one(
        Language::Go,
        "a.go",
        GO_SRC,
        "oldAPI($X)",
        "newAPI($X, nil)",
    );
    assert_eq!(n, 1);
    assert_eq!(
        out,
        "\
package main

// oldAPI(9) is not code
func f() {
\tbanner := \"oldAPI(9)\"
\ta := newAPI(1, nil)
\tb := oldAPI(2, 3)
\tc := newAPI(1)
\t_, _, _, _ = banner, a, b, c
}
"
    );
}

#[test]
fn go_does_not_match_a_similar_call() {
    let (n, _) = one(Language::Go, "a.go", GO_SRC, "oldAPI($X, $Y)", "z($X, $Y)");
    assert_eq!(n, 1, "only the two-argument call");
}

// -------------------------------------------------------------------------- zig

const ZIG_SRC: &str = "\
pub fn f() void {
    // oldApi(9) is not code
    const banner = \"oldApi(9)\";
    const a = oldApi(1);
    const b = oldApi(1, 2);
    _ = banner;
    _ = a;
    _ = b;
}
";

#[test]
fn zig_rewrites_a_call_shape() {
    let (n, out) = one(Language::Zig, "a.zig", ZIG_SRC, "oldApi($X)", "newApi($X)");
    assert_eq!(n, 1);
    assert_eq!(
        out,
        "\
pub fn f() void {
    // oldApi(9) is not code
    const banner = \"oldApi(9)\";
    const a = newApi(1);
    const b = oldApi(1, 2);
    _ = banner;
    _ = a;
    _ = b;
}
"
    );
}

// ------------------------------------------------------------------- typescript

const TS_SRC: &str = "\
// oldApi(9) is not code
const banner = \"oldApi(9)\";
const a = oldApi(1);
const b = oldApi(1, 2);
const c = oldApi(x);
";

#[test]
fn typescript_rewrites_a_call_shape() {
    let (n, out) = one(
        Language::TypeScript,
        "a.ts",
        TS_SRC,
        "oldApi($X)",
        "newApi($X, {})",
    );
    assert_eq!(n, 2);
    assert_eq!(
        out,
        "\
// oldApi(9) is not code
const banner = \"oldApi(9)\";
const a = newApi(1, {});
const b = oldApi(1, 2);
const c = newApi(x, {});
"
    );
}

// -------------------------------------------------------------------------- tsx

const TSX_SRC: &str = "\
// <Old title=\"x\" /> is not code
export const App = () => (
  <div>
    <Old title=\"Home\" />
    <Old title=\"Docs\" />
    <Old title=\"Other\" id=\"o\" />
  </div>
);
";

#[test]
fn tsx_rewrites_a_jsx_element_shape() {
    let (n, out) = one(
        Language::Tsx,
        "a.tsx",
        TSX_SRC,
        "<Old title=\"$T\" />",
        "<New heading=\"$T\" />",
    );
    assert_eq!(n, 2, "the three-attribute element has a different shape");
    assert_eq!(
        out,
        "\
// <Old title=\"x\" /> is not code
export const App = () => (
  <div>
    <New heading=\"Home\" />
    <New heading=\"Docs\" />
    <Old title=\"Other\" id=\"o\" />
  </div>
);
"
    );
}

// ----------------------------------------------------------------------- python

const PY_SRC: &str = "\
# old(9) is not code
banner = \"old(9)\"


def f():
    a = old(1)
    b = old(1, 2)
    return a, b
";

#[test]
fn python_rewrites_a_call_shape() {
    let (n, out) = one(Language::Python, "a.py", PY_SRC, "old($X)", "new($X)");
    assert_eq!(n, 1);
    assert_eq!(
        out,
        "\
# old(9) is not code
banner = \"old(9)\"


def f():
    a = new(1)
    b = old(1, 2)
    return a, b
"
    );
}

// ------------------------------------------------------------------------- bash

const BASH_SRC: &str = "\
#!/usr/bin/env bash
# curl https://z is not code
echo \"curl https://z\"
curl https://a
curl https://b
curl https://c --retry 3
";

#[test]
fn bash_rewrites_a_command_shape() {
    let (n, out) = one(
        Language::Bash,
        "a.sh",
        BASH_SRC,
        "curl $URL",
        "curl --fail $URL",
    );
    assert_eq!(n, 2, "the three-word command has a different shape");
    assert_eq!(
        out,
        "\
#!/usr/bin/env bash
# curl https://z is not code
echo \"curl https://z\"
curl --fail https://a
curl --fail https://b
curl https://c --retry 3
"
    );
}

#[test]
fn bash_double_dollar_matches_a_literal_expansion() {
    // `$X` is a metavariable, so a pattern that means a real shell expansion escapes
    // the sigil. Without this there is no way to write one.
    let src = "curl $BASE\ncurl $OTHER\n";
    let (n, out) = one(Language::Bash, "a.sh", src, "curl $$BASE", "wget $$BASE");
    assert_eq!(n, 1, "only the literal $BASE expansion");
    assert_eq!(out, "wget $BASE\ncurl $OTHER\n");
}

// -------------------------------------------------------------------------- hcl

const HCL_SRC: &str = "\
# var.commented is not code
resource \"aws_s3_bucket\" \"b\" {
  bucket      = var.name
  description = \"var.name\"
  acl         = var.acl
  owner       = data.aws_caller_identity.current.id
}
";

#[test]
fn hcl_rewrites_a_variable_reference() {
    let (n, out) = one(Language::Hcl, "main.tf", HCL_SRC, "var.$X", "local.$X");
    assert_eq!(n, 2);
    assert_eq!(
        out,
        "\
# var.commented is not code
resource \"aws_s3_bucket\" \"b\" {
  bucket      = var.name
  description = \"var.name\"
  acl         = var.acl
  owner       = data.aws_caller_identity.current.id
}
"
        .replace("= var.name", "= local.name")
        .replace("= var.acl", "= local.acl")
    );
}

#[test]
fn hcl_rewrites_an_attribute_shape() {
    let src = "\
resource \"aws_instance\" \"a\" {
  count = 3
  ami   = \"ami-1\"
}
";
    let (n, out) = one(Language::Hcl, "main.tf", src, "count = $N", "for_each = $N");
    assert_eq!(n, 1);
    assert_eq!(
        out,
        "\
resource \"aws_instance\" \"a\" {
  for_each = 3
  ami   = \"ami-1\"
}
"
    );
}

// ------------------------------------------------------------------------- yaml

const YAML_SRC: &str = "\
# image: commented
note: \"image: quoted\"
image: nginx
sidecar:
  image: envoy
imagePullPolicy: Always
";

#[test]
fn yaml_rewrites_a_mapping_pair() {
    let (n, out) = one(
        Language::Yaml,
        "a.yaml",
        YAML_SRC,
        "image: $I",
        "image: registry.example.com/$I",
    );
    assert_eq!(n, 2);
    assert_eq!(
        out,
        "\
# image: commented
note: \"image: quoted\"
image: registry.example.com/nginx
sidecar:
  image: registry.example.com/envoy
imagePullPolicy: Always
"
    );
}

#[test]
fn yaml_does_not_match_a_different_key() {
    let (n, _) = one(Language::Yaml, "a.yaml", YAML_SRC, "tag: $T", "version: $T");
    assert_eq!(n, 0, "no `tag:` key exists");
}

#[test]
fn yaml_binds_a_quoted_scalar_without_its_quotes() {
    let src = "name: \"web\"\nother: web\n";
    let (n, out) = one(
        Language::Yaml,
        "a.yaml",
        src,
        "name: \"$N\"",
        "title: \"$N\"",
    );
    assert_eq!(n, 1, "the unquoted scalar is a different shape");
    assert_eq!(out, "title: \"web\"\nother: web\n");
}

// ------------------------------------------------------------------------- helm

const HELM_CHART: &str = "apiVersion: v2\nname: mychart\nversion: 0.1.0\n";

const HELM_SRC: &str = "\
# image: commented
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .Values.name }}
  note: \"image: quoted\"
spec:
  template:
    spec:
      containers:
        - image: nginx
          imagePullPolicy: Always
";

#[test]
fn helm_rewrites_a_pair_outside_a_template_action() {
    let (n, out) = restructured(
        &[
            ("mychart/Chart.yaml", HELM_CHART),
            ("mychart/templates/deployment.yaml", HELM_SRC),
        ],
        Language::Helm,
        "mychart/templates/deployment.yaml",
        "image: $I",
        "image: registry.example.com/$I",
    );
    assert_eq!(n, 1);
    assert!(
        out.contains("- image: registry.example.com/nginx"),
        "got:\n{out}"
    );
    assert!(
        out.contains("name: {{ .Values.name }}"),
        "the template action is untouched:\n{out}"
    );
}

#[test]
fn helm_does_not_match_a_value_that_is_a_template_action() {
    // `{{ .Values.name }}` is masked out before the YAML parse, so the pair has
    // no value node at all. Matching it would mean rewriting bytes no parse saw.
    let (n, _) = restructured(
        &[
            ("mychart/Chart.yaml", HELM_CHART),
            ("mychart/templates/deployment.yaml", HELM_SRC),
        ],
        Language::Helm,
        "mychart/templates/deployment.yaml",
        "name: $N",
        "title: $N",
    );
    assert_eq!(n, 0, "a masked value is not a match");
}

#[test]
fn helm_does_not_match_a_value_a_template_action_continues() {
    // `web-{{ .Values.suffix }}` parses as the scalar `web-` followed by blanks. The pair looks
    // complete but its value is a lie, so the match must be dropped.
    let src = "metadata:\n  name: web-{{ .Values.suffix }}\n  team: platform\n";
    let (n, out) = restructured(
        &[
            ("mychart/Chart.yaml", HELM_CHART),
            ("mychart/templates/deployment.yaml", src),
        ],
        Language::Helm,
        "mychart/templates/deployment.yaml",
        "name: $N",
        "title: $N",
    );
    assert_eq!(
        n, 0,
        "a value cut short by a template action is not a match"
    );
    assert_eq!(out, src);
}

#[test]
fn helm_refuses_a_pattern_containing_a_template_action() {
    let ws = workspace(&[
        ("mychart/Chart.yaml", HELM_CHART),
        ("mychart/templates/deployment.yaml", HELM_SRC),
    ]);
    let err = restructure::apply(
        &ws.index,
        Language::Helm,
        "name: {{ .Values.name }}",
        "title: x",
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("template action") && message.contains("masked"),
        "the refusal must name the real reason: {message}"
    );
}

// -------------------------------------------------------------------------- css

const CSS_SRC: &str = "\
/* color: green is not code */
.btn {
  color: red;
  margin: 0;
}
.card {
  color: blue;
}
";

#[test]
fn css_rewrites_a_declaration() {
    let (n, out) = one(
        Language::Css,
        "a.css",
        CSS_SRC,
        "color: $C",
        "color: var(--$C)",
    );
    assert_eq!(n, 2);
    assert_eq!(
        out,
        "\
/* color: green is not code */
.btn {
  color: var(--red);
  margin: 0;
}
.card {
  color: var(--blue);
}
"
    );
}

#[test]
fn css_rewrites_a_selector() {
    let (n, out) = one(Language::Css, "a.css", CSS_SRC, ".btn", ".button");
    assert_eq!(n, 1);
    assert!(out.contains(".button {"), "got:\n{out}");
    assert!(out.contains(".card {"), "other selectors untouched:\n{out}");
}

#[test]
fn css_does_not_match_a_different_property() {
    let (n, _) = one(Language::Css, "a.css", CSS_SRC, "padding: $P", "gap: $P");
    assert_eq!(n, 0);
}

#[test]
fn scss_restructures_plain_css_and_scss_only_syntax_alike() {
    // SCSS has its own grammar now, so both halves of the dialect restructure.
    let (n, out) = one(
        Language::Scss,
        "a.scss",
        ".btn {\n  color: red;\n}\n",
        "color: $C",
        "color: var(--$C)",
    );
    assert_eq!(n, 1);
    assert_eq!(out, ".btn {\n  color: var(--red);\n}\n");

    // `$$brand` is an escaped literal `$brand`. SCSS variable syntax, which now
    // parses and so can be rewritten like anything else.
    let (n, out) = one(
        Language::Scss,
        "b.scss",
        ".btn {\n  color: $brand;\n}\n",
        "color: $$brand",
        "color: $$accent",
    );
    assert_eq!(n, 1, "SCSS variables are matchable now");
    assert_eq!(out, ".btn {\n  color: $accent;\n}\n");
}

// ------------------------------------------------------------------------- html

const HTML_SRC: &str = "\
<html>
  <body>
    <!-- <a href=\"/old\">Docs</a> is not markup -->
    <a href=\"/old\">Docs</a>
    <a href=\"/old\">Guide</a>
    <a href=\"/old\" title=\"t\">Other</a>
    <a href=\"/new\">New</a>
  </body>
</html>
";

#[test]
fn html_rewrites_an_element_shape() {
    let (n, out) = one(
        Language::Html,
        "a.html",
        HTML_SRC,
        "<a href=\"/old\">$T</a>",
        "<a href=\"/new\">$T</a>",
    );
    assert_eq!(n, 2, "the two-attribute anchor has a different shape");
    assert_eq!(
        out,
        "\
<html>
  <body>
    <!-- <a href=\"/old\">Docs</a> is not markup -->
    <a href=\"/new\">Docs</a>
    <a href=\"/new\">Guide</a>
    <a href=\"/old\" title=\"t\">Other</a>
    <a href=\"/new\">New</a>
  </body>
</html>
"
    );
}

#[test]
fn html_binds_an_attribute_value() {
    let (n, out) = one(
        Language::Html,
        "a.html",
        HTML_SRC,
        "<a href=\"$H\">$T</a>",
        "<a href=\"$H\" rel=\"noopener\">$T</a>",
    );
    assert_eq!(n, 3);
    assert!(
        out.contains("<a href=\"/old\" rel=\"noopener\">Docs</a>"),
        "got:\n{out}"
    );
    assert!(
        out.contains("<a href=\"/old\" title=\"t\">Other</a>"),
        "the two-attribute anchor is a different shape:\n{out}"
    );
}

// -------------------------------------------------------------------------- xml

const XML_SRC: &str = "\
<project>
  <!-- <dependency scope=\"compile\">junit</dependency> is not markup -->
  <dependency scope=\"compile\">junit</dependency>
  <dependency scope=\"test\">mockito</dependency>
  <dependency>plain</dependency>
</project>
";

#[test]
fn xml_rewrites_an_element_shape() {
    let (n, out) = one(
        Language::Xml,
        "a.xml",
        XML_SRC,
        "<dependency scope=\"$S\">$N</dependency>",
        "<dep scope=\"$S\" pinned=\"true\">$N</dep>",
    );
    assert_eq!(n, 2, "the attribute-less element has a different shape");
    assert_eq!(
        out,
        "\
<project>
  <!-- <dependency scope=\"compile\">junit</dependency> is not markup -->
  <dep scope=\"compile\" pinned=\"true\">junit</dep>
  <dep scope=\"test\" pinned=\"true\">mockito</dep>
  <dependency>plain</dependency>
</project>
"
    );
}

#[test]
fn xml_matches_a_literal_attribute_value() {
    let (n, out) = one(
        Language::Xml,
        "a.xml",
        XML_SRC,
        "<dependency scope=\"test\">$N</dependency>",
        "<testDependency>$N</testDependency>",
    );
    assert_eq!(n, 1, "only the test-scoped dependency");
    assert!(
        out.contains("<testDependency>mockito</testDependency>"),
        "got:\n{out}"
    );
}

// --------------------------------------------------------------------- markdown

const MD_SRC: &str = "\
# Guide

See [the docs](old/url) and [the api](other/url).

```
[the docs](old/url)
```

## Install
";

#[test]
fn markdown_rewrites_a_link_destination() {
    let (n, out) = one(
        Language::Markdown,
        "a.md",
        MD_SRC,
        "[$T](old/url)",
        "[$T](new/url)",
    );
    assert_eq!(
        n, 1,
        "the other link and the fenced code block are not matches"
    );
    assert_eq!(
        out,
        "\
# Guide

See [the docs](new/url) and [the api](other/url).

```
[the docs](old/url)
```

## Install
"
    );
}

#[test]
fn markdown_rewrites_a_heading() {
    let (n, out) = one(Language::Markdown, "a.md", MD_SRC, "## $T", "### $T");
    assert_eq!(n, 1, "only the level-two heading");
    assert!(out.contains("### Install"), "got:\n{out}");
    assert!(
        out.contains("# Guide"),
        "the level-one heading is untouched:\n{out}"
    );
}

// ------------------------------------------------------------------ every cell

/// Every column of PLAN.md's feature × language matrix, with a pattern that is
/// realistic for that language. This is the promise the matrix makes.
#[test]
fn every_matrix_language_restructures() {
    let cases: &[(Language, &str, &str, &str, &str, &str)] = &[
        (
            Language::Rust,
            "a.rs",
            "fn f() { let x = old(1); }\n",
            "old($X)",
            "new($X)",
            "fn f() { let x = new(1); }\n",
        ),
        (
            Language::Go,
            "a.go",
            // `new` is a Go builtin the grammar expects a type argument for, so the
            // replacement is a plain name: the reparse gate rejects the alternative.
            "package p\n\nfunc f() { oldAPI(1) }\n",
            "oldAPI($X)",
            "newAPI($X)",
            "package p\n\nfunc f() { newAPI(1) }\n",
        ),
        (
            Language::Zig,
            "a.zig",
            "pub fn f() void {\n    _ = old(1);\n}\n",
            "old($X)",
            "new($X)",
            "pub fn f() void {\n    _ = new(1);\n}\n",
        ),
        (
            Language::TypeScript,
            "a.ts",
            "const x = old(1);\n",
            "old($X)",
            "new($X)",
            "const x = new(1);\n",
        ),
        (
            Language::Tsx,
            "a.tsx",
            "const A = () => <Old title=\"h\" />;\n",
            "<Old title=\"$T\" />",
            "<New title=\"$T\" />",
            "const A = () => <New title=\"h\" />;\n",
        ),
        (
            Language::Python,
            "a.py",
            "x = old(1)\n",
            "old($X)",
            "new($X)",
            "x = new(1)\n",
        ),
        (
            Language::Bash,
            "a.sh",
            "curl https://a\n",
            "curl $U",
            "curl --fail $U",
            "curl --fail https://a\n",
        ),
        (
            Language::Hcl,
            "main.tf",
            "locals {\n  b = var.name\n}\n",
            "var.$X",
            "local.$X",
            "locals {\n  b = local.name\n}\n",
        ),
        (
            Language::Yaml,
            "a.yaml",
            "image: nginx\n",
            "image: $I",
            "image: ghcr.io/$I",
            "image: ghcr.io/nginx\n",
        ),
        (
            Language::Css,
            "a.css",
            ".b {\n  color: red;\n}\n",
            "color: $C",
            "color: var(--$C)",
            ".b {\n  color: var(--red);\n}\n",
        ),
        (
            Language::Html,
            "a.html",
            "<p><a href=\"/old\">D</a></p>\n",
            "<a href=\"/old\">$T</a>",
            "<a href=\"/new\">$T</a>",
            "<p><a href=\"/new\">D</a></p>\n",
        ),
        (
            Language::Xml,
            "a.xml",
            "<r><c id=\"a\">t</c></r>\n",
            "<c id=\"$I\">$T</c>",
            "<d id=\"$I\">$T</d>",
            "<r><d id=\"a\">t</d></r>\n",
        ),
        (
            Language::Markdown,
            "a.md",
            "See [d](old/url).\n",
            "[$T](old/url)",
            "[$T](new/url)",
            "See [d](new/url).\n",
        ),
    ];

    for (language, file, src, pattern, template, expected) in cases {
        let (n, out) = one(*language, file, src, pattern, template);
        assert_eq!(n, 1, "{language}: expected exactly one match");
        assert_eq!(out, *expected, "{language} rewrote to:\n{out}");
    }

    // Helm needs a chart around it to be detected at all.
    let (n, out) = restructured(
        &[
            ("mychart/Chart.yaml", HELM_CHART),
            ("mychart/templates/d.yaml", "image: nginx\n"),
        ],
        Language::Helm,
        "mychart/templates/d.yaml",
        "image: $I",
        "image: ghcr.io/$I",
    );
    assert_eq!(n, 1, "helm: expected exactly one match");
    assert_eq!(out, "image: ghcr.io/nginx\n");
}

/// Strings and comments are not code, in every language that has them.
#[test]
fn no_language_matches_inside_a_string_or_comment() {
    let cases: &[(Language, &str, &str, &str)] = &[
        (
            Language::Rust,
            "a.rs",
            "fn f() {\n    // old(1)\n    let s = \"old(1)\";\n}\n",
            "old($X)",
        ),
        (
            Language::Go,
            "a.go",
            "package p\n\n// old(1)\nvar s = \"old(1)\"\n",
            "old($X)",
        ),
        (
            Language::Zig,
            "a.zig",
            "pub fn f() void {\n    // old(1)\n    const s = \"old(1)\";\n    _ = s;\n}\n",
            "old($X)",
        ),
        (
            Language::TypeScript,
            "a.ts",
            "// old(1)\nconst s = \"old(1)\";\n",
            "old($X)",
        ),
        (
            Language::Tsx,
            "a.tsx",
            "// <Old title=\"h\" />\nconst s = \"<Old title='h' />\";\n",
            "<Old title=\"$T\" />",
        ),
        (
            Language::Python,
            "a.py",
            "# old(1)\ns = \"old(1)\"\n",
            "old($X)",
        ),
        (
            Language::Bash,
            "a.sh",
            "# curl https://a\necho \"curl https://a\"\n",
            "curl $U",
        ),
        (
            Language::Hcl,
            "main.tf",
            "locals {\n  # var.name\n  a = \"var.name\"\n}\n",
            "var.$X",
        ),
        (
            Language::Yaml,
            "a.yaml",
            "# image: nginx\nnote: \"image: nginx\"\n",
            "image: $I",
        ),
        (
            Language::Css,
            "a.css",
            "/* color: red */\n.b {\n  content: \"color: red\";\n}\n",
            "color: $C",
        ),
        (
            Language::Html,
            "a.html",
            "<p><!-- <a href=\"/old\">D</a> --></p>\n",
            "<a href=\"/old\">$T</a>",
        ),
        (
            Language::Xml,
            "a.xml",
            "<r><!-- <c id=\"a\">t</c> --></r>\n",
            "<c id=\"$I\">$T</c>",
        ),
        (
            // Markdown has no strings; a fenced code block is its "not prose".
            Language::Markdown,
            "a.md",
            "```\n[d](old/url)\n```\n",
            "[$T](old/url)",
        ),
    ];

    for (language, file, src, pattern) in cases {
        let (n, out) = one(*language, file, src, pattern, "SHOULD_NOT_APPEAR");
        assert_eq!(
            n, 0,
            "{language} matched inside a string or comment:\n{out}"
        );
        assert_eq!(out, *src, "{language} rewrote a string or comment");
    }
}

// ------------------------------------------------------------------- semantics

#[test]
fn a_repeated_metavariable_binds_consistently_in_yaml() {
    let src = "a:\n  x: same\n  y: same\nb:\n  x: one\n  y: two\n";
    let (n, out) = one(Language::Yaml, "a.yaml", src, "x: $V\ny: $V", "pair: $V");
    assert_eq!(n, 1, "only the mapping whose two values are equal");
    assert!(out.contains("pair: same"), "got:\n{out}");
    assert!(out.contains("x: one"), "got:\n{out}");
}

#[test]
fn a_pattern_that_is_only_a_metavariable_is_refused_in_every_language() {
    for language in [
        Language::Rust,
        Language::Bash,
        Language::Hcl,
        Language::Yaml,
        Language::Css,
        Language::Html,
        Language::Xml,
        Language::Markdown,
    ] {
        let ws = workspace(&[("a.rs", "fn f() {}\n")]);
        let err = restructure::apply(&ws.index, language, "$X", "$X").unwrap_err();
        assert!(
            err.to_string().contains("would match everything"),
            "{language}: {err}"
        );
    }
}

#[test]
fn an_unparseable_fragment_is_refused_in_the_language_s_own_words() {
    // The error used to enumerate the fragment wrappers, which describes the parsing
    // machinery instead of the mistake in the pattern.
    let ws = workspace(&[("a.css", ".b { color: red; }\n")]);
    let err = restructure::apply(&ws.index, Language::Css, "} not css {", "x").unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("'} not css {' is not valid css; check for unbalanced brackets."),
        "the mistake is named: {message}"
    );
    assert!(!message.contains("wrapper"), "no wrapper jargon: {message}");
}

#[test]
fn only_the_requested_language_is_rewritten() {
    let ws = workspace(&[
        ("a.py", "x = old(1)\n"),
        ("a.ts", "const x = old(1);\n"),
        ("a.yaml", "image: nginx\n"),
    ]);
    let result = restructure::apply(&ws.index, Language::Python, "old($X)", "new($X)").unwrap();
    assert_eq!(result.matches.len(), 1);
    assert!(result.matches[0].0.ends_with("a.py"));
}

#[test]
fn a_statement_pattern_works_where_the_wrapper_is_empty() {
    // Python, shell and YAML wrap a fragment in nothing at all, so the statement the
    // pattern writes is the outermost node. The descent that strips wrapper-introduced
    // statement containers used to strip that one too, leaving the fragment starting
    // six bytes inside itself, every statement pattern in those languages was
    // rejected as unparseable. Descending is only right when the child begins where
    // the container does; `raise` does not.
    let src = "\
def f(e):
    try:
        g()
    except ValueError as e:
        raise Invalid(e)
";
    let (n, out) = one(
        Language::Python,
        "a.py",
        src,
        "raise Invalid($X)",
        "raise Invalid($X) from None",
    );
    assert_eq!(n, 1);
    assert!(out.contains("raise Invalid(e) from None"), "got:\n{out}");
}

#[test]
fn an_expression_pattern_still_matches_only_the_expression() {
    // The other half of the same rule: `g()` must not start matching whole statements.
    let src = "def f():\n    a = g()\n    return a\n";
    let (n, out) = one(Language::Python, "a.py", src, "g()", "h()");
    assert_eq!(n, 1);
    assert!(out.contains("a = h()"), "got:\n{out}");
}
