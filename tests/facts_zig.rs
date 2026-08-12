//! Zig fact-extraction tests: what `queries/zig/facts.scm` reports.

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

fn zig(src: &str) -> FileFacts {
    facts(Language::Zig, src)
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
    let src = "fn helper() void {}\npub fn add(a: i32, b: i32) i32 {\n    return a + b;\n}\n";
    let f = zig(src);
    assert_eq!(names_of(&f, SymbolKind::Function), vec!["helper", "add"]);

    let add = sym(&f, "add");
    assert_eq!(add.name_span.text(src), "add");
    assert!(add.full_span.contains(add.name_span));
    assert!(add.full_span.text(src).starts_with("pub fn add"));
    assert!(add.full_span.text(src).ends_with('}'));
    assert_eq!(add.qualifier, None);
}

#[test]
fn pub_marks_a_function_exported() {
    let src = "fn private() void {}\npub fn public() void {}\n";
    let f = zig(src);
    assert!(!sym(&f, "private").exported);
    assert!(sym(&f, "public").exported);
}

#[test]
fn export_and_extern_also_mark_external_visibility() {
    let src = "export fn abi() callconv(.c) void {}\nextern var errno: c_int;\n";
    let f = zig(src);
    assert!(sym(&f, "abi").exported, "`export fn` is externally visible");
    // `extern` declarations have no initialiser, so they need their own pattern.
    let errno = sym(&f, "errno");
    assert_eq!(errno.kind, SymbolKind::Variable);
    assert!(errno.exported);
}

#[test]
fn parameters_are_definitions_inside_the_function_scope() {
    let src = "const unrelated: u8 = 0;\npub fn add(a: i32, b: i32) i32 {\n    return a + b;\n}\n";
    let f = zig(src);
    let a = sym(&f, "a");
    assert_eq!(a.kind, SymbolKind::Parameter);
    assert_eq!(a.name_span.text(src), "a");
    assert!(!a.exported);

    let file_scope = f.scope_at(0).unwrap();
    assert_ne!(a.scope, file_scope);
    assert!(f.scope_chain(a.scope).contains(&file_scope));
}

// ----------------------------------------------------------------- test decls

#[test]
fn a_string_named_test_drops_the_quotes_from_its_name_span() {
    let src = "test \"adds two numbers\" {}\n";
    let f = zig(src);
    let t = sym(&f, "adds two numbers");
    assert_eq!(t.kind, SymbolKind::Function);
    // A rename must rewrite the text, never the surrounding quotes.
    assert_eq!(t.name_span.text(src), "adds two numbers");
    assert_eq!(t.full_span.text(src), "test \"adds two numbers\" {}");
}

#[test]
fn a_decl_test_references_the_declaration_it_exercises() {
    let src = "fn thing() void {}\ntest thing {}\n";
    let f = zig(src);
    // `test thing` names an existing declaration instead of introducing one, so
    // `thing` keeps a single definition site and the test block is a use of it.
    let things: Vec<_> = f.symbols.iter().filter(|s| s.name == "thing").collect();
    assert_eq!(things.len(), 1, "got {things:#?}");
    assert_eq!(things[0].kind, SymbolKind::Function);

    let r = f.references.iter().find(|r| r.name == "thing").unwrap();
    assert!(r.span.start > src.find("test ").unwrap());
}

// -------------------------------------------------------------- declarations

#[test]
fn const_struct_is_a_single_struct_definition() {
    let src = "pub const Point = struct {\n    x: i32,\n    y: i32,\n};\n";
    let f = zig(src);
    let p = sym(&f, "Point");
    assert_eq!(p.kind, SymbolKind::Struct);
    assert!(p.exported);
    // The name a rename rewrites is the const's identifier.
    assert_eq!(p.name_span.text(src), "Point");
    // …and the definition is the whole statement. It is not just the struct body.
    assert!(p.full_span.text(src).starts_with("pub const Point"));
    assert!(p.full_span.text(src).ends_with(';'));
}

#[test]
fn each_container_form_yields_exactly_one_symbol_of_the_right_kind() {
    let src = "const S = struct { a: u8 };\n\
               const U = union { a: u8, b: u16 };\n\
               const E = enum { red, green };\n\
               const O = opaque { fn f() void {} };\n\
               const Err = error{ OutOfMemory };\n";
    let f = zig(src);
    assert_eq!(sym(&f, "S").kind, SymbolKind::Struct);
    // The model has no union kind; a Zig union is a record, so it lands on struct.
    assert_eq!(sym(&f, "U").kind, SymbolKind::Struct);
    assert_eq!(sym(&f, "E").kind, SymbolKind::Enum);
    assert_eq!(sym(&f, "O").kind, SymbolKind::Struct);
    // An error set is a closed set of named values, like an enum.
    assert_eq!(sym(&f, "Err").kind, SymbolKind::Enum);
}

#[test]
fn empty_container_bodies_do_not_parse() {
    // Grammar limitation, not a query one: tree-sitter-zig 1.1.2 requires at least
    // one member inside a container, so `struct {}`, valid Zig, fails to parse.
    // Recorded here so the day the grammar is fixed, this test fails loudly.
    for src in ["const Z = struct {};\n", "const O = opaque {};\n"] {
        let parsed = Parsers::new().parse(Language::Zig, src).unwrap();
        assert!(parsed.has_errors(), "{src:?} unexpectedly parsed cleanly");
        // The grammar flags this subtree without emitting an ERROR node. The parser
        // falls back to the innermost node that reports an error, so the breakage is
        // still visible to the edit engine's before/after comparison.
        assert!(
            !parsed.error_spans().is_empty(),
            "{src:?} must report a span, or an edit that breaks a file this way \
             would be accepted"
        );
    }
}

#[test]
fn qualified_container_forms_are_still_single_definitions() {
    let src = "const P = packed struct { a: u8 };\n\
               const X = extern struct { a: u8 };\n\
               const T = union(enum) { a: u8 };\n\
               const B = enum(u8) { red };\n";
    let f = zig(src);
    assert_eq!(sym(&f, "P").kind, SymbolKind::Struct);
    assert_eq!(sym(&f, "X").kind, SymbolKind::Struct);
    assert_eq!(sym(&f, "T").kind, SymbolKind::Struct);
    assert_eq!(sym(&f, "B").kind, SymbolKind::Enum);
}

#[test]
fn an_error_set_merge_stays_an_ordinary_constant() {
    // `error{A} || error{B}` is a binary expression. It is not an error-set declaration.
    // It must not be swallowed by the container rules nor lost between them.
    let src = "const A = error{One};\nconst B = error{Two};\nconst Both = A || B;\nconst Inline = error{X} || error{Y};\n";
    let f = zig(src);
    assert_eq!(sym(&f, "A").kind, SymbolKind::Enum);
    assert_eq!(sym(&f, "Both").kind, SymbolKind::Constant);
    assert_eq!(sym(&f, "Inline").kind, SymbolKind::Constant);
}

#[test]
fn const_and_var_map_to_constant_and_variable() {
    let src = "pub const MAX: u32 = 10;\nvar counter: u32 = 0;\npub var shared: u32 = 1;\n";
    let f = zig(src);
    let max = sym(&f, "MAX");
    assert_eq!(max.kind, SymbolKind::Constant);
    assert!(max.exported);
    assert_eq!(max.name_span.text(src), "MAX");
    assert_eq!(max.full_span.text(src), "pub const MAX: u32 = 10;");

    assert_eq!(sym(&f, "counter").kind, SymbolKind::Variable);
    assert!(!sym(&f, "counter").exported);
    assert!(sym(&f, "shared").exported);
}

#[test]
fn a_var_initialiser_is_not_mistaken_for_a_second_declaration() {
    // `var a = b;` has two identifier children; only the one before `=` is declared.
    let src = "const b: u32 = 1;\nvar a = b;\n";
    let f = zig(src);
    assert_eq!(names_of(&f, SymbolKind::Variable), vec!["a"]);
    assert_eq!(names_of(&f, SymbolKind::Constant), vec!["b"]);
}

#[test]
fn function_locals_are_constants_or_variables_by_keyword() {
    // Zig spells an immutable local `const`; the keyword, not the position, decides.
    let src = "fn f() u32 {\n    const total: u32 = 1;\n    var acc: u32 = 0;\n    acc += total;\n    return acc;\n}\n";
    let f = zig(src);
    assert_eq!(sym(&f, "total").kind, SymbolKind::Constant);
    assert_eq!(sym(&f, "acc").kind, SymbolKind::Variable);
    assert!(!sym(&f, "total").exported);
}

// ---------------------------------------------------------- members & methods

#[test]
fn functions_inside_a_container_become_qualified_methods() {
    let src = "pub const Point = struct {\n\
               \x20   x: i32,\n\
               \n\
               \x20   pub fn init(v: i32) Point {\n\
               \x20       return Point{ .x = v };\n\
               \x20   }\n\
               \n\
               \x20   fn hidden(self: Point) i32 {\n\
               \x20       return self.x;\n\
               \x20   }\n\
               };\n";
    let f = zig(src);
    let init = sym(&f, "init");
    assert_eq!(init.kind, SymbolKind::Method);
    assert_eq!(init.qualifier.as_deref(), Some("Point"));
    assert_eq!(init.qualified_name(), "Point::init");
    assert!(init.exported);

    let hidden = sym(&f, "hidden");
    assert_eq!(hidden.qualified_name(), "Point::hidden");
    assert!(!hidden.exported);

    // The container reuses the type's own definition; `Point` is declared once.
    let point = sym(&f, "Point");
    assert_eq!(point.kind, SymbolKind::Struct);
}

#[test]
fn container_fields_are_qualified_fields() {
    let src = "const Point = struct {\n    x: i32,\n    y: i32 = 0,\n};\n";
    let f = zig(src);
    let x = sym(&f, "x");
    assert_eq!(x.kind, SymbolKind::Field);
    assert_eq!(x.qualified_name(), "Point::x");
    assert_eq!(x.name_span.text(src), "x");
    assert_eq!(x.full_span.text(src), "x: i32");
    assert_eq!(sym(&f, "y").qualified_name(), "Point::y");
}

#[test]
fn enum_members_are_qualified_fields() {
    let src = "const Color = enum { red, green };\n";
    let f = zig(src);
    assert_eq!(sym(&f, "red").kind, SymbolKind::Field);
    assert_eq!(sym(&f, "red").qualified_name(), "Color::red");
    assert_eq!(sym(&f, "green").qualified_name(), "Color::green");
}

#[test]
fn error_set_members_are_qualified_fields_spanning_only_their_name() {
    let src = "const MyError = error{ OutOfMemory, Invalid };\n";
    let f = zig(src);
    let oom = sym(&f, "OutOfMemory");
    assert_eq!(oom.kind, SymbolKind::Field);
    assert_eq!(oom.qualified_name(), "MyError::OutOfMemory");
    // A set member is nothing but its name, so both spans coincide.
    assert_eq!(oom.name_span.text(src), "OutOfMemory");
    assert_eq!(oom.full_span, oom.name_span);
}

#[test]
fn payload_captures_are_variable_definitions() {
    let src = "fn f(maybe: ?u32) u32 {\n    if (maybe) |value| {\n        return value;\n    }\n    return 0;\n}\n";
    let f = zig(src);
    let v = sym(&f, "value");
    assert_eq!(v.kind, SymbolKind::Variable);
    assert_eq!(v.name_span.text(src), "value");
}

// ---------------------------------------------------------------- references

#[test]
fn calls_are_reported_as_call_references() {
    let src = "fn a() void {}\n\
               const P = struct {\n\
               \x20   fn m(self: P) void { _ = self; }\n\
               };\n\
               fn b(p: P) void {\n\
               \x20   a();\n\
               \x20   p.m();\n\
               }\n";
    let f = zig(src);
    let call = |name: &str| {
        f.references
            .iter()
            .filter(|r| r.name == name && r.kind == ReferenceKind::Call)
            .count()
    };
    assert_eq!(call("a"), 1, "plain call");
    assert_eq!(call("m"), 1, "method call through a value");
}

#[test]
fn field_access_is_a_field_reference() {
    let src = "const P = struct {\n    x: i32,\n    fn get(self: P) i32 { return self.x; }\n};\n";
    let f = zig(src);
    let x_refs: Vec<_> = f.references.iter().filter(|r| r.name == "x").collect();
    assert_eq!(x_refs.len(), 1, "got {x_refs:?}");
    assert_eq!(x_refs[0].kind, ReferenceKind::Field);
    assert!(x_refs[0].span.start > src.find("return").unwrap());
}

#[test]
fn type_positions_are_type_references() {
    let src = "const P = struct { x: u8 };\n\
               fn direct(p: P) P { return p; }\n\
               fn pointer(p: *P) void { _ = p; }\n\
               fn optional(p: ?P) void { _ = p; }\n";
    let f = zig(src);
    let type_refs = f
        .references
        .iter()
        .filter(|r| r.name == "P" && r.kind == ReferenceKind::Type)
        .count();
    // Parameter type, return type, `*P` and `?P`.
    assert_eq!(type_refs, 4, "refs: {:?}", f.references);
}

#[test]
fn struct_initializers_name_their_type() {
    let src = "const P = struct { x: u8 };\nfn make() P { return P{ .x = 1 }; }\n";
    let f = zig(src);
    let init_offset = src.rfind("P{").unwrap();
    let r = f
        .reference_at(init_offset)
        .expect("initializer names a type");
    assert_eq!(r.name, "P");
    assert_eq!(r.kind, ReferenceKind::Type);
}

#[test]
fn definition_identifiers_are_not_also_references() {
    let src = "fn alpha() void {}\nfn beta() void {\n    alpha();\n}\n";
    let f = zig(src);
    let alpha_refs: Vec<_> = f.references.iter().filter(|r| r.name == "alpha").collect();
    assert_eq!(alpha_refs.len(), 1, "got {alpha_refs:?}");
    assert_eq!(alpha_refs[0].kind, ReferenceKind::Call);
    assert!(alpha_refs[0].span.start > src.find("fn beta").unwrap());
}

#[test]
fn a_container_name_is_declared_once_and_referenced_elsewhere() {
    let src = "const P = struct { x: u8 };\nfn make() P { return P{ .x = 1 }; }\n";
    let f = zig(src);
    let p = sym(&f, "P");
    // The declaration site is a definition, never also a reference.
    assert!(f.reference_at(p.name_span.start).is_none());
    // Both later mentions are references.
    assert_eq!(f.references.iter().filter(|r| r.name == "P").count(), 2);
}

#[test]
fn references_start_unresolved() {
    let f = zig("fn a() void { b(); }\n");
    let r = f.references.iter().find(|r| r.name == "b").unwrap();
    assert_eq!(r.target, None);
    assert_eq!(r.confidence, Confidence::NameOnly);
}

// ------------------------------------------------------------------- imports

#[test]
fn import_binds_a_name_to_a_path() {
    let src = "const std = @import(\"std\");\n";
    let f = zig(src);
    assert_eq!(f.imports.len(), 1);
    let i = &f.imports[0];
    // The path is reported without its quotes.
    assert_eq!(i.path, "std");
    assert_eq!(i.names.len(), 1);
    assert_eq!(i.names[0].local, "std");
    assert_eq!(i.names[0].original, "std");
    assert!(!i.names[0].is_aliased());
    assert_eq!(i.span.text(src), "const std = @import(\"std\");");

    // The binding is also an ordinary constant, because that is what it is.
    assert_eq!(sym(&f, "std").kind, SymbolKind::Constant);
}

#[test]
fn a_relative_import_keeps_its_path() {
    let src = "const util = @import(\"../lib/util.zig\");\n";
    let f = zig(src);
    assert_eq!(f.imports[0].path, "../lib/util.zig");
    assert_eq!(f.imports[0].names[0].local, "util");
}

#[test]
fn importing_one_member_records_the_original_name() {
    let src = "const alloc = @import(\"std\").mem;\n";
    let f = zig(src);
    assert_eq!(f.imports.len(), 1);
    let i = &f.imports[0];
    assert_eq!(i.path, "std");
    assert_eq!(i.names[0].local, "alloc");
    assert_eq!(i.names[0].original, "mem");
    assert!(i.names[0].is_aliased());
}

#[test]
fn a_pub_import_is_exported() {
    let src = "pub const std = @import(\"std\");\n";
    let f = zig(src);
    assert_eq!(f.imports.len(), 1);
    assert!(sym(&f, "std").exported);
}

// -------------------------------------------------------------------- scopes

#[test]
fn blocks_nest_inside_functions_which_nest_inside_the_file() {
    let src = "fn outer() void {\n    const x: u32 = 1;\n    {\n        const y: u32 = 2;\n        _ = y;\n    }\n    _ = x;\n}\n";
    let f = zig(src);
    let inner = f.scope_at(src.find("const y").unwrap()).unwrap();
    let outer = f.scope_at(src.find("const x").unwrap()).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
    let file = f.scope_at(0).unwrap();
    assert!(f.scope_chain(inner).contains(&file));
}

// ------------------------------------------------------------ whole-file pass

#[test]
fn a_realistic_file_extracts_without_duplicate_definitions() {
    let src = r#"const std = @import("std");
const mem = @import("std").mem;

pub const Error = error{ OutOfMemory, Invalid };

pub const Color = enum { red, green, blue };

pub const Point = struct {
    x: i32,
    y: i32 = 0,

    pub fn init(x: i32, y: i32) Point {
        return Point{ .x = x, .y = y };
    }

    pub fn sum(self: *const Point) i32 {
        return self.x + self.y;
    }
};

pub var origin: Point = .{ .x = 0 };
const MAX: i32 = 100;

pub fn clamp(v: i32) i32 {
    if (v > MAX) {
        return MAX;
    }
    return v;
}

test "clamp caps at MAX" {
    const p = Point.init(1, 2);
    _ = p.sum();
    _ = clamp(200);
    _ = std.mem.eql;
    _ = mem;
}
"#;
    let f = zig(src);

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

    assert_eq!(sym(&f, "Error").kind, SymbolKind::Enum);
    assert_eq!(sym(&f, "Color").kind, SymbolKind::Enum);
    assert_eq!(sym(&f, "Point").kind, SymbolKind::Struct);
    assert_eq!(sym(&f, "origin").kind, SymbolKind::Variable);
    assert_eq!(sym(&f, "MAX").kind, SymbolKind::Constant);
    assert_eq!(sym(&f, "clamp").kind, SymbolKind::Function);
    assert_eq!(sym(&f, "init").qualified_name(), "Point::init");
    assert_eq!(sym(&f, "sum").qualified_name(), "Point::sum");
    assert_eq!(f.imports.len(), 2);

    // `MAX` is defined once and used twice.
    assert_eq!(f.references.iter().filter(|r| r.name == "MAX").count(), 2);
    // Calls survive the whole pass.
    assert_eq!(
        f.references
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        vec!["init", "sum", "clamp"]
    );
}
