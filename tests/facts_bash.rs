//! Bash fact-extraction tests: what `queries/bash/facts.scm` actually yields.
//!
//! Bash is dynamically scoped and has no declarations to lean on, so several of
//! these tests pin down deliberate heuristics rather than language guarantees.
//! Where the query language cannot express something, the test says so.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Bash, src).unwrap();
    // A sample that does not parse would make every other assertion meaningless.
    assert!(
        !parsed.has_errors(),
        "sample has parse errors at {:?}",
        parsed.error_spans()
    );
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

/// The one symbol with this name. Fails if a definition was captured twice,
/// which is the failure mode of overlapping query patterns.
fn one<'a>(f: &'a FileFacts, name: &str) -> &'a Symbol {
    let found: Vec<_> = f.symbols.iter().filter(|s| s.name == name).collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one `{name}`, got {found:?}"
    );
    found[0]
}

fn names_of(f: &FileFacts, kind: SymbolKind) -> Vec<&str> {
    f.symbols
        .iter()
        .filter(|s| s.kind == kind)
        .map(|s| s.name.as_str())
        .collect()
}

const RICH: &str = r#"#!/usr/bin/env bash
set -euo pipefail

source ./lib/util.sh
. /etc/profile

TOP=1
export EXPORTED=2
readonly FROZEN=3
declare -i counter=0

greet() {
  local name=$1
  local greeting="hello"
  echo "${greeting} ${name}"
}

function farewell {
  local who="$1"
  printf '%s\n' "bye ${who}"
}

main() {
  greet world
  farewell world
  if [ -n "${TOP}" ]; then
    branch=1
  else
    branch=2
  fi
  for item in a b c; do
    seen="${item}"
  done
  echo "${branch} ${seen} ${counter}"
}

main "$@"
"#;

#[test]
fn rich_sample_parses_without_errors() {
    let parsed = Parsers::new().parse(Language::Bash, RICH).unwrap();
    assert!(
        !parsed.has_errors(),
        "parse errors at {:?}",
        parsed.error_spans()
    );
}

#[test]
fn no_definition_is_captured_twice() {
    // `local x=1` is both a declaration and an assignment: the declaration
    // patterns and the plain-assignment patterns must not both claim it, or a
    // rename would emit two edits over the same bytes.
    let f = facts(RICH);
    let mut spans: Vec<_> = f.symbols.iter().map(|s| s.name_span).collect();
    let before = spans.len();
    spans.sort();
    spans.dedup();
    assert_eq!(
        before,
        spans.len(),
        "duplicate definitions in {:?}",
        f.symbols
    );
}

#[test]
fn both_function_syntaxes_are_captured() {
    let src = "posix() {\n  echo a\n}\nfunction keyword {\n  echo b\n}\nfunction hybrid() {\n  echo c\n}\n";
    let f = facts(src);
    assert_eq!(
        names_of(&f, SymbolKind::Function),
        vec!["posix", "keyword", "hybrid"]
    );

    // name_span is the identifier alone — the bytes a rename rewrites.
    let posix = one(&f, "posix");
    assert_eq!(posix.name_span.text(src), "posix");
    assert_eq!(posix.full_span.text(src), "posix() {\n  echo a\n}");

    let keyword = one(&f, "keyword");
    assert_eq!(keyword.name_span.text(src), "keyword");
    assert_eq!(
        keyword.full_span.text(src),
        "function keyword {\n  echo b\n}"
    );
}

#[test]
fn functions_are_never_marked_exported() {
    // Bash has no visibility control over functions: every one is global once
    // its definition has run. @export is reserved for `export`ed variables.
    let f = facts(RICH);
    for name in ["greet", "farewell", "main"] {
        assert!(!one(&f, name).exported, "{name}");
    }
}

#[test]
fn a_nested_function_is_contained_by_its_outer_function() {
    // Bash has no method-like construct, so there are no @container patterns and
    // nothing gets qualified — nesting shows up as containment only.
    let src = "outer() {\n  inner() { echo hi; }\n  inner\n}\n";
    let f = facts(src);
    let inner = one(&f, "inner");
    assert_eq!(inner.kind, SymbolKind::Function);
    assert_eq!(inner.qualifier, None);
    assert_eq!(
        inner
            .container
            .map(|id| f.symbol(id).unwrap().name.as_str()),
        Some("outer")
    );
}

#[test]
fn plain_assignments_are_variables() {
    let src = "TOP=1\nlower=2\n";
    let f = facts(src);
    assert_eq!(names_of(&f, SymbolKind::Variable), vec!["TOP", "lower"]);
    let top = one(&f, "TOP");
    assert_eq!(top.name_span.text(src), "TOP");
    assert_eq!(top.full_span.text(src), "TOP=1");
    assert!(!top.exported);
}

#[test]
fn only_export_marks_a_variable_exported() {
    let src = "PLAIN=1\nexport SHARED=2\nreadonly FROZEN=3\ndeclare -i counted=4\nlocal scoped=5\nexport BARE\n";
    let f = facts(src);
    assert!(one(&f, "SHARED").exported);
    assert!(one(&f, "BARE").exported);
    for name in ["PLAIN", "FROZEN", "counted", "scoped"] {
        assert!(!one(&f, name).exported, "{name}");
    }
    // The declaration keyword is not part of the assignment node…
    assert_eq!(one(&f, "SHARED").full_span.text(src), "SHARED=2");
    // …but a declaration with no value has nothing else to point at.
    assert_eq!(one(&f, "BARE").full_span.text(src), "export BARE");
    assert_eq!(one(&f, "BARE").name_span.text(src), "BARE");
}

#[test]
fn local_declarations_are_captured_once_with_and_without_a_value() {
    let src = "f() {\n  local named=1\n  local bare\n}\n";
    let f = facts(src);
    assert_eq!(names_of(&f, SymbolKind::Variable), vec!["named", "bare"]);
    assert_eq!(one(&f, "named").full_span.text(src), "named=1");
    assert_eq!(one(&f, "bare").full_span.text(src), "local bare");
}

#[test]
fn assignments_are_captured_in_every_statement_position() {
    let src = "top=0\nf() {\n  in_body=1\n}\n( in_subshell=2 )\nif true; then\n  in_then=3\nelif true; then\n  in_elif=4\nelse\n  in_else=5\nfi\nwhile false; do\n  in_loop=6\ndone\ncase x in\n  x) in_case=7 ;;\nesac\ntrue && in_list=8\nPREFIX=9 env\na=10 b=11\nout=$( inner=12 )\n";
    let f = facts(src);
    let vars = names_of(&f, SymbolKind::Variable);
    for expected in [
        "top",
        "in_body",
        "in_subshell",
        "in_then",
        "in_elif",
        "in_else",
        "in_loop",
        "in_case",
        "in_list",
        "PREFIX",
        "a",
        "b",
        "out",
        "inner",
    ] {
        assert!(vars.contains(&expected), "missing {expected} in {vars:?}");
    }
}

#[test]
fn a_for_loop_binds_its_variable_without_swallowing_the_body() {
    let src = "for item in a b; do\n  seen=$item\ndone\n";
    let f = facts(src);
    let item = one(&f, "item");
    assert_eq!(item.kind, SymbolKind::Variable);
    assert_eq!(item.name_span.text(src), "item");
    // The definition is the identifier, so the loop body is not "inside" it.
    assert_eq!(item.full_span, item.name_span);
    assert_eq!(one(&f, "seen").container, None);
}

#[test]
fn a_c_style_for_loop_binds_its_initializer() {
    let src = "for (( i=0; i<3; i++ )); do\n  echo \"$i\"\ndone\n";
    let f = facts(src);
    let i = one(&f, "i");
    assert_eq!(i.kind, SymbolKind::Variable);
    assert_eq!(i.name_span.text(src), "i");
    assert_eq!(i.full_span.text(src), "i=0");
}

#[test]
fn positional_parameters_are_not_captured() {
    // `$1` is a variable_name node, but it has no definition site and cannot be
    // renamed, so it is deliberately left out rather than reported as a use of a
    // variable called "1".
    let src = "f() {\n  local name=$1\n  echo \"$2 $name\"\n}\n";
    let f = facts(src);
    let names: Vec<_> = f.references.iter().map(|r| r.name.as_str()).collect();
    assert!(!names.contains(&"1"), "got {names:?}");
    assert!(!names.contains(&"2"), "got {names:?}");
    assert!(names.contains(&"name"));
}

#[test]
fn command_invocations_are_call_references() {
    let src = "greet() {\n  echo hi\n}\ngreet\n";
    let f = facts(src);
    let greet_refs: Vec<_> = f.references.iter().filter(|r| r.name == "greet").collect();
    assert_eq!(greet_refs.len(), 1, "got {greet_refs:?}");
    assert_eq!(greet_refs[0].kind, ReferenceKind::Call);
    // Definition and use are distinct byte ranges; only the use is a reference.
    assert!(greet_refs[0].span.start > src.find("greet() {").unwrap());

    // External commands look exactly the same; resolution is the index's job.
    let echo = f.references.iter().find(|r| r.name == "echo").unwrap();
    assert_eq!(echo.kind, ReferenceKind::Call);
    assert_eq!(echo.target, None);
    assert_eq!(echo.confidence, Confidence::NameOnly);
}

#[test]
fn every_expansion_form_is_an_identifier_reference() {
    let src =
        "NAME=1\narr=(x y)\nn=0\necho $NAME\necho \"${NAME}\"\necho \"${arr[0]}\"\n(( n++ ))\n";
    let f = facts(src);
    let refs: Vec<_> = f
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Identifier)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(refs, vec!["NAME", "NAME", "arr", "n"]);
}

#[test]
fn a_definition_identifier_is_not_also_a_reference() {
    let src = "X=1\necho $X\n";
    let f = facts(src);
    let x_refs: Vec<_> = f.references.iter().filter(|r| r.name == "X").collect();
    assert_eq!(x_refs.len(), 1, "got {x_refs:?}");
    assert!(x_refs[0].span.start > src.find("X=1").unwrap());
}

#[test]
fn source_and_dot_are_imports() {
    let src = "source ./lib.sh\n. /etc/profile\n";
    let f = facts(src);
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["./lib.sh", "/etc/profile"]);
    assert!(f.imports.iter().all(|i| !i.is_glob && i.names.is_empty()));
    assert_eq!(f.imports[0].span.text(src), "source ./lib.sh");
}

#[test]
fn quoted_source_paths_lose_their_quotes() {
    let src = "source \"$DIR/util.sh\"\nsource 'plain.sh'\n";
    let f = facts(src);
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["$DIR/util.sh", "plain.sh"]);
    // The expansion inside the quoted path is still a variable use.
    assert!(f.references.iter().any(|r| r.name == "DIR"));
}

#[test]
fn a_concatenated_source_path_is_captured_but_not_unquoted_cleanly() {
    // Known shortcoming, in the extractor rather than the query: Import paths are
    // unquoted by trimming quote characters off the ends, which cannot handle a
    // path built from several pieces. The import itself, and its span, are right.
    let src = "source \"$DIR\"/lib.sh\n";
    let f = facts(src);
    assert_eq!(f.imports.len(), 1);
    assert_eq!(f.imports[0].span.text(src), "source \"$DIR\"/lib.sh");
    assert_eq!(f.imports[0].path, "$DIR\"/lib.sh");
}

#[test]
fn a_command_named_source_is_also_a_call_reference() {
    // `source` is an ordinary builtin invocation as well as an import, and both
    // facts are reported.
    let f = facts("source ./lib.sh\n");
    assert_eq!(f.imports.len(), 1);
    let r = f.references.iter().find(|r| r.name == "source").unwrap();
    assert_eq!(r.kind, ReferenceKind::Call);
}

#[test]
fn function_bodies_and_subshells_are_scopes() {
    // Bash scoping is dynamic, so this tree is a lexical approximation: a name
    // used in a function may at run time resolve to a caller's variable.
    let src = "outer() {\n  inside=1\n}\ntoplevel=2\n( subshelled=3 )\n";
    let f = facts(src);
    let body = f.scope_at(src.find("inside=1").unwrap()).unwrap();
    let top = f.scope_at(src.find("toplevel=2").unwrap()).unwrap();
    let sub = f.scope_at(src.find("subshelled=3").unwrap()).unwrap();
    assert_ne!(body, top);
    assert_ne!(sub, top);
    assert!(f.scope_chain(body).contains(&top));
    assert!(f.scope_chain(sub).contains(&top));
}

#[test]
fn symbol_and_reference_lookup_by_offset() {
    let src = "greet() {\n  echo hi\n}\ngreet\n";
    let f = facts(src);
    let def_offset = src.find("greet").unwrap() + 1;
    assert_eq!(
        f.symbol_at(def_offset).map(|s| s.name.as_str()),
        Some("greet")
    );
    let call_offset = src.rfind("greet").unwrap() + 1;
    assert_eq!(
        f.reference_at(call_offset).map(|r| r.name.as_str()),
        Some("greet")
    );
}

#[test]
fn the_rich_sample_yields_the_expected_facts() {
    let f = facts(RICH);
    let functions = names_of(&f, SymbolKind::Function);
    assert_eq!(functions, vec!["greet", "farewell", "main"]);

    let vars = names_of(&f, SymbolKind::Variable);
    for expected in [
        "TOP", "EXPORTED", "FROZEN", "counter", "name", "greeting", "who", "branch", "item", "seen",
    ] {
        assert!(vars.contains(&expected), "missing {expected} in {vars:?}");
    }

    assert_eq!(
        f.imports
            .iter()
            .map(|i| i.path.as_str())
            .collect::<Vec<_>>(),
        vec!["./lib/util.sh", "/etc/profile"]
    );

    // Calls to workspace functions are indistinguishable from external commands
    // at this layer; both are Call references awaiting resolution.
    let calls: Vec<_> = f
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .map(|r| r.name.as_str())
        .collect();
    assert!(calls.contains(&"greet"), "got {calls:?}");
    assert!(calls.contains(&"main"), "got {calls:?}");
    assert!(calls.contains(&"printf"), "got {calls:?}");
}
