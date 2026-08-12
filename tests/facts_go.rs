//! Go fact-extraction tests: what `queries/go/facts.scm` actually reports.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "sample must parse cleanly, errors at {:?}",
        parsed.error_spans()
    );
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

fn go(src: &str) -> FileFacts {
    facts(Language::Go, src)
}

fn sym<'a>(f: &'a FileFacts, name: &str) -> &'a Symbol {
    let hits: Vec<_> = f.symbols.iter().filter(|s| s.name == name).collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one symbol named {name}, got {hits:#?}"
    );
    hits[0]
}

fn names_of(f: &FileFacts, kind: SymbolKind) -> Vec<&str> {
    f.symbols
        .iter()
        .filter(|s| s.kind == kind)
        .map(|s| s.name.as_str())
        .collect()
}

// ------------------------------------------------------------------ functions

#[test]
fn functions_are_found_with_exact_name_spans() {
    let src = "package p\n\nfunc Add(a int, b int) int { return a + b }\nfunc helper() {}\n";
    let f = go(src);
    assert_eq!(names_of(&f, SymbolKind::Function), vec!["Add", "helper"]);

    let add = sym(&f, "Add");
    // The name span is what a rename rewrites: the identifier and nothing else.
    assert_eq!(add.name_span.text(src), "Add");
    assert!(add.full_span.contains(add.name_span));
    assert_eq!(
        add.full_span.text(src),
        "func Add(a int, b int) int { return a + b }"
    );
    assert_eq!(add.qualifier, None);
    assert_eq!(add.qualified_name(), "Add");
}

#[test]
fn capitalisation_decides_export() {
    let src = "package p\n\nfunc Exported() {}\nfunc unexported() {}\n";
    let f = go(src);
    assert!(sym(&f, "Exported").exported);
    assert!(!sym(&f, "unexported").exported);
}

#[test]
fn export_test_applies_to_every_package_level_kind() {
    let src = "package p\n\
               \n\
               type Open struct{ Field int; hidden int }\n\
               type closed interface{}\n\
               type Alias = Open\n\
               const Big = 1\n\
               const small = 2\n\
               var Wide int\n\
               var narrow int\n";
    let f = go(src);
    for exported in ["Open", "Alias", "Big", "Wide", "Field"] {
        assert!(sym(&f, exported).exported, "{exported} should be exported");
    }
    for private in ["closed", "small", "narrow", "hidden"] {
        assert!(!sym(&f, private).exported, "{private} should be unexported");
    }
}

#[test]
fn parameters_and_locals_are_never_exported() {
    // Capitalised locals are still function-private; the export test must not leak
    // into scopes where it has no meaning.
    let src = "package p\n\nfunc f(Arg int, rest ...string) {\n\tLocal := Arg\n\t_ = Local\n}\n";
    let f = go(src);
    let arg = sym(&f, "Arg");
    assert_eq!(arg.kind, SymbolKind::Parameter);
    assert!(!arg.exported);
    let rest = sym(&f, "rest");
    assert_eq!(rest.kind, SymbolKind::Parameter);
    let local = sym(&f, "Local");
    assert_eq!(local.kind, SymbolKind::Variable);
    assert!(!local.exported);
}

// -------------------------------------------------------------------- methods

#[test]
fn value_receiver_qualifies_the_method() {
    let src =
        "package p\n\ntype Point struct{ X int }\n\nfunc (p Point) Area() int { return p.X }\n";
    let f = go(src);
    let area = sym(&f, "Area");
    assert_eq!(area.kind, SymbolKind::Method);
    assert_eq!(area.qualifier.as_deref(), Some("Point"));
    assert_eq!(area.qualified_name(), "Point::Area");
    assert_eq!(area.name_span.text(src), "Area");
}

#[test]
fn pointer_receiver_qualifies_by_the_pointee_type() {
    let src = "package p\n\ntype Point struct{ X int }\n\nfunc (p *Point) Scale(f int) {}\n";
    let f = go(src);
    let scale = sym(&f, "Scale");
    assert_eq!(scale.kind, SymbolKind::Method);
    // `*Point` must qualify as `Point`, not `*Point`.
    assert_eq!(scale.qualified_name(), "Point::Scale");
}

#[test]
fn generic_receivers_qualify_by_the_bare_type_name() {
    let src = "package p\n\
               \n\
               type Stack[T any] struct{ items []T }\n\
               \n\
               func (s *Stack[T]) Push(v T) {}\n\
               func (s Stack[T]) Len() int { return 0 }\n";
    let f = go(src);
    assert_eq!(sym(&f, "Push").qualified_name(), "Stack::Push");
    assert_eq!(sym(&f, "Len").qualified_name(), "Stack::Len");
}

#[test]
fn the_receiver_type_is_a_reference_not_a_second_definition() {
    let src = "package p\n\ntype Point struct{}\n\nfunc (p *Point) M() {}\n";
    let f = go(src);
    // Exactly one definition of Point, so a rename has exactly one definition site.
    let point = sym(&f, "Point");
    assert_eq!(point.kind, SymbolKind::Struct);

    // …and the receiver mention is a reference, so a rename still rewrites it.
    let receiver = src.rfind("Point").unwrap();
    let r = f
        .reference_at(receiver)
        .expect("receiver type is a reference");
    assert_eq!(r.name, "Point");
    assert_eq!(r.kind, ReferenceKind::Type);
}

#[test]
fn interface_methods_are_qualified_by_the_interface() {
    let src = "package p\n\ntype Shape interface {\n\tArea() float64\n\tscale(f float64)\n}\n";
    let f = go(src);
    let shape = sym(&f, "Shape");
    assert_eq!(shape.kind, SymbolKind::Interface);

    let area = sym(&f, "Area");
    assert_eq!(area.kind, SymbolKind::Method);
    assert_eq!(area.qualified_name(), "Shape::Area");
    assert!(area.exported);

    let scale = sym(&f, "scale");
    assert_eq!(scale.qualified_name(), "Shape::scale");
    assert!(!scale.exported);
}

// --------------------------------------------------------------------- types

#[test]
fn each_type_declaration_form_yields_exactly_one_symbol_of_the_right_kind() {
    let src = "package p\n\
               \n\
               type S struct{}\n\
               type I interface{}\n\
               type A = S\n\
               type Meters float64\n\
               type Handler func(int) error\n\
               type Table map[string]int\n";
    let f = go(src);
    assert_eq!(sym(&f, "S").kind, SymbolKind::Struct);
    assert_eq!(sym(&f, "I").kind, SymbolKind::Interface);
    // Both an alias and a defined type land in the one `type` kind the model has.
    assert_eq!(sym(&f, "A").kind, SymbolKind::TypeAlias);
    assert_eq!(sym(&f, "Meters").kind, SymbolKind::TypeAlias);
    assert_eq!(sym(&f, "Handler").kind, SymbolKind::TypeAlias);
    assert_eq!(sym(&f, "Table").kind, SymbolKind::TypeAlias);
}

#[test]
fn struct_fields_are_qualified_and_share_a_declaration() {
    let src = "package p\n\ntype Point struct {\n\tX, Y int\n\tlabel string\n}\n";
    let f = go(src);
    let x = sym(&f, "X");
    let y = sym(&f, "Y");
    assert_eq!(x.kind, SymbolKind::Field);
    assert_eq!(x.qualified_name(), "Point::X");
    assert_eq!(y.qualified_name(), "Point::Y");
    // One `X, Y int` declaration, two definitions with distinct name spans.
    assert_eq!(x.name_span.text(src), "X");
    assert_eq!(y.name_span.text(src), "Y");
    assert_eq!(x.full_span, y.full_span);
    assert_eq!(x.full_span.text(src), "X, Y int");
    assert_eq!(sym(&f, "label").qualified_name(), "Point::label");
}

#[test]
fn grouped_declarations_report_one_symbol_per_spec() {
    let src = "package p\n\nconst (\n\tA = 1\n\tb = 2\n)\n\nvar (\n\tC int\n\td int\n)\n";
    let f = go(src);
    assert_eq!(names_of(&f, SymbolKind::Constant), vec!["A", "b"]);
    assert_eq!(names_of(&f, SymbolKind::Variable), vec!["C", "d"]);
    // The definition span is the spec, not the whole `const ( ... )` block.
    assert_eq!(sym(&f, "A").full_span.text(src), "A = 1");
}

#[test]
fn multi_name_var_spec_yields_one_symbol_per_name() {
    let src = "package p\n\nvar x, y = 1, 2\n";
    let f = go(src);
    assert_eq!(names_of(&f, SymbolKind::Variable), vec!["x", "y"]);
    assert_eq!(sym(&f, "x").name_span.text(src), "x");
    assert_eq!(sym(&f, "y").name_span.text(src), "y");
}

#[test]
fn package_clause_is_a_module_definition() {
    let src = "package widgets\n";
    let f = go(src);
    let p = sym(&f, "widgets");
    assert_eq!(p.kind, SymbolKind::Module);
    assert_eq!(p.name_span.text(src), "widgets");
    // Importers can name the package, so it is visible outside the file.
    assert!(p.exported);
}

#[test]
fn locals_come_from_short_declarations_and_range_clauses() {
    let src = "package p\n\nfunc f(xs []int) {\n\tn := 0\n\tfor i, v := range xs {\n\t\tn = i + v\n\t}\n}\n";
    let f = go(src);
    for local in ["n", "i", "v"] {
        let s = sym(&f, local);
        assert_eq!(s.kind, SymbolKind::Variable, "{local}");
    }
}

#[test]
fn type_parameters_are_parameter_definitions() {
    let src = "package p\n\nfunc Map[T any, U any](in []T) []U { return nil }\n";
    let f = go(src);
    assert_eq!(sym(&f, "T").kind, SymbolKind::Parameter);
    assert_eq!(sym(&f, "U").kind, SymbolKind::Parameter);
    assert_eq!(sym(&f, "T").name_span.text(src), "T");
}

// ---------------------------------------------------------------- references

#[test]
fn calls_are_reported_as_call_references() {
    let src = "package p\n\nimport \"fmt\"\n\nfunc a() {}\n\nfunc b(p Point) {\n\ta()\n\tfmt.Println(1)\n\tp.Scale(2)\n}\n\ntype Point struct{}\n\nfunc (p Point) Scale(n int) {}\n";
    let f = go(src);
    let call = |name: &str| {
        f.references
            .iter()
            .filter(|r| r.name == name && r.kind == ReferenceKind::Call)
            .count()
    };
    assert_eq!(call("a"), 1, "plain call");
    assert_eq!(call("Println"), 1, "selector call through a package");
    assert_eq!(call("Scale"), 1, "method call through a value");
}

#[test]
fn field_access_is_a_field_reference() {
    let src = "package p\n\ntype Point struct{ X int }\n\nfunc f(p Point) int { return p.X }\n";
    let f = go(src);
    let x_refs: Vec<_> = f.references.iter().filter(|r| r.name == "X").collect();
    assert_eq!(x_refs.len(), 1, "got {x_refs:?}");
    assert_eq!(x_refs[0].kind, ReferenceKind::Field);
    assert!(x_refs[0].span.start > src.find("return").unwrap());
}

#[test]
fn composite_literal_keys_are_field_references() {
    let src = "package p\n\ntype Point struct{ X int }\n\nfunc f() Point { return Point{X: 1} }\n";
    let f = go(src);
    let x_refs: Vec<_> = f.references.iter().filter(|r| r.name == "X").collect();
    assert_eq!(x_refs.len(), 1, "got {x_refs:?}");
    assert_eq!(x_refs[0].kind, ReferenceKind::Field);
}

#[test]
fn type_positions_are_type_references() {
    let src = "package p\n\ntype Point struct{}\n\nfunc f(p Point) Point { return p }\n";
    let f = go(src);
    let point_refs: Vec<_> = f.references.iter().filter(|r| r.name == "Point").collect();
    // Parameter type and result type, the declaration itself is not a reference.
    assert_eq!(point_refs.len(), 2, "got {point_refs:?}");
    assert!(point_refs.iter().all(|r| r.kind == ReferenceKind::Type));
}

#[test]
fn definition_identifiers_are_not_also_references() {
    let src = "package p\n\nfunc alpha() {}\n\nfunc beta() { alpha() }\n";
    let f = go(src);
    let alpha_refs: Vec<_> = f.references.iter().filter(|r| r.name == "alpha").collect();
    assert_eq!(alpha_refs.len(), 1, "got {alpha_refs:?}");
    assert_eq!(alpha_refs[0].kind, ReferenceKind::Call);
    assert!(alpha_refs[0].span.start > src.find("func beta").unwrap());
}

#[test]
fn references_start_unresolved() {
    let f = go("package p\n\nfunc a() { b() }\n");
    let r = f.references.iter().find(|r| r.name == "b").unwrap();
    assert_eq!(r.target, None);
    assert_eq!(r.confidence, Confidence::NameOnly);
}

// ------------------------------------------------------------------- imports

#[test]
fn single_import_captures_the_unquoted_path() {
    let src = "package p\n\nimport \"fmt\"\n";
    let f = go(src);
    assert_eq!(f.imports.len(), 1);
    assert_eq!(f.imports[0].path, "fmt");
    assert_eq!(f.imports[0].alias, None);
    assert!(!f.imports[0].is_glob);
}

#[test]
fn grouped_imports_report_one_entry_per_spec() {
    let src =
        "package p\n\nimport (\n\t\"os\"\n\tstr \"strings\"\n\t. \"math\"\n\t_ \"embed\"\n)\n";
    let f = go(src);
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["os", "strings", "math", "embed"]);

    let by_path = |p: &str| f.imports.iter().find(|i| i.path == p).unwrap();
    assert_eq!(by_path("os").alias, None);
    assert_eq!(by_path("strings").alias.as_deref(), Some("str"));
    // A dot import dumps every exported name into file scope, glob semantics.
    assert!(by_path("math").is_glob);
    assert_eq!(by_path("math").alias, None);
    // A blank import binds nothing; the recorded alias says so.
    assert_eq!(by_path("embed").alias.as_deref(), Some("_"));
    assert!(!by_path("embed").is_glob);

    // Each import's span covers just its own spec, not the whole block.
    assert_eq!(by_path("strings").span.text(src), "str \"strings\"");
}

#[test]
fn import_paths_keep_their_slashes() {
    let src = "package p\n\nimport \"github.com/foo/bar\"\n";
    let f = go(src);
    assert_eq!(f.imports[0].path, "github.com/foo/bar");
}

// -------------------------------------------------------------------- scopes

#[test]
fn function_bodies_and_blocks_nest() {
    let src =
        "package p\n\nfunc outer() {\n\tx := 1\n\t{\n\t\ty := 2\n\t\t_ = y\n\t}\n\t_ = x\n}\n";
    let f = go(src);
    let inner = f.scope_at(src.find("y := 2").unwrap()).unwrap();
    let outer = f.scope_at(src.find("x := 1").unwrap()).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
    // Every scope chain terminates at the file scope.
    let file = f.scope_at(src.find("package").unwrap()).unwrap();
    assert!(f.scope_chain(inner).contains(&file));
}

#[test]
fn a_parameter_lives_in_its_function_scope_not_the_file_scope() {
    let src = "package p\n\nfunc f(a int) {\n\t_ = a\n}\n";
    let f = go(src);
    let file = f.scope_at(src.find("package").unwrap()).unwrap();
    let param = sym(&f, "a").scope;
    assert_ne!(param, file);
    assert!(f.scope_chain(param).contains(&file));
}

// ------------------------------------------------------------ whole-file pass

#[test]
fn a_realistic_file_extracts_without_duplicate_definitions() {
    let src = r#"package main

import (
	"fmt"
	str "strings"
)

type Shape interface {
	Area() float64
}

type Rect struct {
	W, H float64
	name string
}

func (r Rect) Area() float64 {
	return r.W * r.H
}

func (r *Rect) Rename(n string) {
	r.name = str.TrimSpace(n)
}

func main() {
	r := Rect{W: 1, H: 2}
	fmt.Println(r.Area())
}
"#;
    let f = go(src);

    // No two definitions may claim the same identifier bytes: a rename must have a
    // single definition site per symbol.
    let mut seen = std::collections::HashMap::new();
    for s in &f.symbols {
        if let Some(prev) = seen.insert(s.name_span, s) {
            panic!(
                "duplicate definition at {:?}: {:?} and {:?}",
                s.name_span, prev, s
            );
        }
    }

    // `Area` is declared twice: once on the interface, once on the struct. They are
    // distinct symbols distinguished by their qualifier.
    let areas: Vec<_> = f.symbols.iter().filter(|s| s.name == "Area").collect();
    assert_eq!(areas.len(), 2);
    let mut quals: Vec<_> = areas.iter().map(|s| s.qualified_name()).collect();
    quals.sort();
    assert_eq!(quals, vec!["Rect::Area", "Shape::Area"]);

    // And the call site resolves to the name, not to either declaration.
    let call_area = f
        .references
        .iter()
        .filter(|r| r.name == "Area" && r.kind == ReferenceKind::Call)
        .count();
    assert_eq!(call_area, 1);
}
