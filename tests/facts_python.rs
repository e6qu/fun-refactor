//! Python fact-extraction tests: what `queries/python/facts.scm` yields.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Python, src).unwrap();
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

/// The one symbol with this name.
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

const RICH: &str = r#"
import os
import os.path as p
from m import a, b as c
from . import rel_thing
from .sub import other
from wild import *
from __future__ import annotations

CONST = 1
_HIDDEN_CONST = 2
value = 3
_private = 4

type Alias = list[int]


def free(x, y: int = 1, *args, **kwargs):
    local = x
    if x:
        nested_local = y
    return local, nested_local


async def coro():
    pass


def _helper():
    pass


class Widget(Base):
    kind = "w"
    count: int = 0
    _secret = None

    def render(self, target):
        global CONST
        helper = target.build()
        return helper

    def _internal(self):
        pass


@decorator
@mod.decorator(1)
def decorated():
    free(1)
    Widget().render(None)
"#;

#[test]
fn rich_sample_parses_without_errors() {
    let parsed = Parsers::new().parse(Language::Python, RICH).unwrap();
    assert!(
        !parsed.has_errors(),
        "parse errors at {:?}",
        parsed.error_spans()
    );
}

#[test]
fn no_definition_is_captured_twice() {
    // Several patterns can match one node (a class is both @definition.class and @container; an
    // `export`-style variant exists for most kinds).
    let f = facts(RICH);
    let mut spans: Vec<_> = f.symbols.iter().map(|s| (s.name_span, s.kind)).collect();
    let before = spans.len();
    spans.sort();
    spans.dedup_by_key(|(span, _)| *span);
    assert_eq!(
        before,
        spans.len(),
        "duplicate definitions in {:?}",
        f.symbols
    );
}

#[test]
fn functions_are_found_with_identifier_only_name_spans() {
    let src = "def alpha():\n    pass\n\n\nasync def beta(x):\n    return x\n";
    let f = facts(src);
    assert_eq!(names_of(&f, SymbolKind::Function), vec!["alpha", "beta"]);

    let alpha = one(&f, "alpha");
    // name_span is what a rename rewrites: the identifier and nothing else.
    assert_eq!(alpha.name_span.text(src), "alpha");
    assert!(alpha.full_span.contains(alpha.name_span));
    assert_eq!(alpha.full_span.text(src), "def alpha():\n    pass");

    // `async` belongs to the definition, so full_span starts at it.
    assert_eq!(
        one(&f, "beta").full_span.text(src),
        "async def beta(x):\n    return x"
    );
}

#[test]
fn a_decorated_definition_is_captured_and_excludes_its_decorators() {
    // Documented query choice: the captured node is the inner function_definition, so full_span
    // starts at `def`.
    let src = "@deco\ndef target():\n    pass\n";
    let f = facts(src);
    let target = one(&f, "target");
    assert_eq!(target.kind, SymbolKind::Function);
    assert_eq!(target.name_span.text(src), "target");
    assert_eq!(target.full_span.text(src), "def target():\n    pass");
    assert!(!target.full_span.text(src).contains("@deco"));
}

#[test]
fn a_class_is_a_symbol_and_also_qualifies_its_methods() {
    let src = "class Widget:\n    def render(self):\n        pass\n";
    let f = facts(src);

    // The class stays a single renameable symbol even though the same node is
    // also captured as a @container.
    let widget = one(&f, "Widget");
    assert_eq!(widget.kind, SymbolKind::Class);
    assert_eq!(widget.name_span.text(src), "Widget");
    assert_eq!(widget.qualifier, None, "a class must not qualify itself");

    let render = one(&f, "render");
    assert_eq!(render.kind, SymbolKind::Method);
    assert_eq!(render.qualifier.as_deref(), Some("Widget"));
    assert_eq!(render.qualified_name(), "Widget::render");
    assert_eq!(render.name_span.text(src), "render");
    // The method is nested inside the class symbol.
    assert_eq!(
        render
            .container
            .map(|id| f.symbol(id).unwrap().name.as_str()),
        Some("Widget")
    );
}

#[test]
fn a_top_level_function_is_not_a_method() {
    let f = facts("def free():\n    pass\n");
    let free = one(&f, "free");
    assert_eq!(free.kind, SymbolKind::Function);
    assert_eq!(free.qualifier, None);
    assert_eq!(free.qualified_name(), "free");
}

#[test]
fn a_function_nested_in_a_method_inherits_the_class_qualifier() {
    // Known behaviour, not a claim that it is ideal: the extractor qualifies by the innermost
    // @container, and only the class is a container.
    let src = "class C:\n    def m(self):\n        def inner():\n            pass\n        return inner\n";
    let f = facts(src);
    let inner = one(&f, "inner");
    assert_eq!(inner.kind, SymbolKind::Method);
    assert_eq!(inner.qualifier.as_deref(), Some("C"));
    // Its containing symbol is still the method it is written in.
    assert_eq!(
        inner
            .container
            .map(|id| f.symbol(id).unwrap().name.as_str()),
        Some("m")
    );
}

#[test]
fn nested_classes_are_qualified_by_the_outer_class() {
    let f = facts("class Outer:\n    class Inner:\n        pass\n");
    let inner = one(&f, "Inner");
    assert_eq!(inner.kind, SymbolKind::Class);
    assert_eq!(inner.qualified_name(), "Outer::Inner");
}

#[test]
fn every_parameter_form_is_captured() {
    let src = "def f(plain, typed: int, dflt=1, both: str = 'x', *args, **kwargs):\n    pass\n";
    let f = facts(src);
    assert_eq!(
        names_of(&f, SymbolKind::Parameter),
        vec!["plain", "typed", "dflt", "both", "args", "kwargs"]
    );
    // The name span excludes the annotation, the default and the splat marker.
    assert_eq!(one(&f, "both").name_span.text(src), "both");
    assert_eq!(one(&f, "both").full_span.text(src), "both: str = 'x'");
    assert_eq!(one(&f, "args").name_span.text(src), "args");
    assert_eq!(one(&f, "kwargs").name_span.text(src), "kwargs");
}

#[test]
fn annotated_splat_parameters_are_captured() {
    let f = facts("def f(*args: int, **kwargs: str):\n    pass\n");
    assert_eq!(names_of(&f, SymbolKind::Parameter), vec!["args", "kwargs"]);
}

#[test]
fn lambda_parameters_are_captured() {
    let f = facts("fn = lambda z: z\n");
    assert_eq!(names_of(&f, SymbolKind::Parameter), vec!["z"]);
}

#[test]
fn module_assignments_split_into_constants_and_variables() {
    let src = "MAX_SIZE = 10\nDEBUG = False\nsetting = 1\n";
    let f = facts(src);
    assert_eq!(
        names_of(&f, SymbolKind::Constant),
        vec!["MAX_SIZE", "DEBUG"]
    );
    assert_eq!(names_of(&f, SymbolKind::Variable), vec!["setting"]);
    assert_eq!(one(&f, "MAX_SIZE").name_span.text(src), "MAX_SIZE");
    assert_eq!(one(&f, "MAX_SIZE").full_span.text(src), "MAX_SIZE = 10");
}

#[test]
fn class_body_assignments_are_fields() {
    let src = "class C:\n    kind = 'w'\n    count: int = 0\n";
    let f = facts(src);
    assert_eq!(names_of(&f, SymbolKind::Field), vec!["kind", "count"]);
    let count = one(&f, "count");
    assert_eq!(count.qualifier.as_deref(), Some("C"));
    assert_eq!(count.name_span.text(src), "count");
    assert_eq!(count.full_span.text(src), "count: int = 0");
}

#[test]
fn the_underscore_convention_decides_exportedness() {
    // Python has no visibility keyword; a leading underscore means "internal".
    let f = facts(RICH);
    for public in ["free", "coro", "Widget", "CONST", "value", "Alias", "kind"] {
        assert!(one(&f, public).exported, "{public} should be exported");
    }
    for private in [
        "_helper",
        "_HIDDEN_CONST",
        "_private",
        "_secret",
        "_internal",
    ] {
        assert!(
            !one(&f, private).exported,
            "{private} should not be exported"
        );
    }
}

#[test]
fn dunder_methods_are_not_exported() {
    // A consequence of the underscore rule, stated so the behaviour is deliberate.
    let f = facts("class C:\n    def __init__(self):\n        pass\n");
    assert!(!one(&f, "__init__").exported);
}

#[test]
fn locals_are_captured_in_function_bodies_and_nested_blocks() {
    let src = "def f(flag):\n    direct = 1\n    if flag:\n        in_if = 2\n    else:\n        in_else = 3\n    for i in []:\n        in_for = 4\n    while flag:\n        in_while = 5\n    try:\n        in_try = 6\n    except Exception:\n        in_except = 7\n    finally:\n        in_finally = 8\n    with open('f') as fh:\n        in_with = 9\n    return direct\n";
    let f = facts(src);
    let vars = names_of(&f, SymbolKind::Variable);
    for expected in [
        "direct",
        "in_if",
        "in_else",
        "in_for",
        "in_while",
        "in_try",
        "in_except",
        "in_finally",
        "in_with",
    ] {
        assert!(vars.contains(&expected), "missing {expected} in {vars:?}");
    }
    assert_eq!(one(&f, "in_if").full_span.text(src), "in_if = 2");
}

#[test]
fn loop_with_and_walrus_bindings_are_variables() {
    let src = "for i in range(3):\n    pass\nfor k, v in d.items():\n    pass\nwith open('f') as fh:\n    pass\ntry:\n    pass\nexcept ValueError as err:\n    pass\nif (n := 1) > 0:\n    pass\nsq = [j for j in x]\n";
    let f = facts(src);
    for name in ["i", "k", "v", "fh", "err", "n", "j"] {
        let sym = one(&f, name);
        assert_eq!(sym.kind, SymbolKind::Variable, "{name}");
        // These bind exactly one identifier, so the definition is that identifier:
        // full_span must not swallow the loop body.
        assert_eq!(sym.full_span, sym.name_span, "{name}");
        assert_eq!(sym.name_span.text(src), name);
    }
}

#[test]
fn a_non_identifier_as_target_does_not_define_a_symbol() {
    // `with cm as self.attr` is legal Python but binds an attribute, not a new
    // name, defining a symbol called "self.attr" would be nonsense.
    let f = facts("with cm() as self.attr:\n    pass\n");
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
    assert!(f.references.iter().any(|r| r.name == "attr"));
}

#[test]
fn tuple_unpacking_in_an_assignment_is_a_known_gap() {
    // `a, b = pair` binds two names, but the assignment patterns only match a single-identifier
    // left-hand side, so no symbol is defined.
    let src = "a, b = pair\n";
    let f = facts(src);
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
    let names: Vec<_> = f.references.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "pair"]);
}

#[test]
fn type_alias_statements_are_type_symbols() {
    let src = "type Alias = list[int]\n";
    let f = facts(src);
    let alias = one(&f, "Alias");
    assert_eq!(alias.kind, SymbolKind::TypeAlias);
    assert_eq!(alias.name_span.text(src), "Alias");
}

#[test]
fn calls_are_call_references() {
    let src = "def caller():\n    helper()\n    obj.method()\n";
    let f = facts(src);
    let helper = f.references.iter().find(|r| r.name == "helper").unwrap();
    assert_eq!(helper.kind, ReferenceKind::Call);
    // The attribute of `obj.method()` is the callee, so it beats the plain
    // field reading of the same identifier.
    let method = f.references.iter().find(|r| r.name == "method").unwrap();
    assert_eq!(method.kind, ReferenceKind::Call);
    // The receiver is an ordinary identifier use.
    let obj = f.references.iter().find(|r| r.name == "obj").unwrap();
    assert_eq!(obj.kind, ReferenceKind::Identifier);
}

#[test]
fn attribute_access_without_a_call_is_a_field_reference() {
    let f = facts("def f(o):\n    return o.attr\n");
    let attr = f.references.iter().find(|r| r.name == "attr").unwrap();
    assert_eq!(attr.kind, ReferenceKind::Field);
}

#[test]
fn decorators_reference_the_callable_they_name() {
    let src = "@deco\n@mod.other\n@deco2(1)\ndef f():\n    pass\n";
    let f = facts(src);
    for name in ["deco", "other", "deco2"] {
        let r = f.references.iter().find(|r| r.name == name).unwrap();
        assert_eq!(r.kind, ReferenceKind::Call, "{name}");
    }
}

#[test]
fn annotations_and_superclasses_are_type_references() {
    let src = "class C(Base):\n    def m(self, x: Widget) -> Result:\n        pass\n";
    let f = facts(src);
    for name in ["Base", "Widget", "Result"] {
        let r = f.references.iter().find(|r| r.name == name).unwrap();
        assert_eq!(r.kind, ReferenceKind::Type, "{name}");
    }
}

#[test]
fn global_and_nonlocal_names_are_references_not_definitions() {
    // `global CONST` rebinds a name defined elsewhere; capturing it as a
    // definition would give one variable two definition sites.
    let src = "CONST = 1\n\n\ndef f():\n    global CONST\n    print(CONST)\n\n\ndef outer():\n    v = 1\n\n    def inner():\n        nonlocal v\n        print(v)\n";
    let f = facts(src);
    let const_defs: Vec<_> = f.symbols.iter().filter(|s| s.name == "CONST").collect();
    assert_eq!(const_defs.len(), 1, "got {const_defs:?}");
    assert_eq!(const_defs[0].kind, SymbolKind::Constant);

    let global_offset = src.find("global CONST").unwrap() + "global ".len();
    let r = f
        .reference_at(global_offset)
        .expect("global name is a reference");
    assert_eq!(r.name, "CONST");
    assert_eq!(r.span.text(src), "CONST");

    let nonlocal_offset = src.find("nonlocal v").unwrap() + "nonlocal ".len();
    assert_eq!(
        f.reference_at(nonlocal_offset).map(|r| r.name.as_str()),
        Some("v")
    );
}

#[test]
fn assigning_a_global_inside_a_function_still_looks_like_a_local() {
    // Known limitation.
    let src = "CONST = 1\n\n\ndef f():\n    global CONST\n    CONST = 2\n";
    let f = facts(src);
    let defs: Vec<_> = f
        .symbols
        .iter()
        .filter(|s| s.name == "CONST")
        .map(|s| s.kind)
        .collect();
    assert_eq!(defs, vec![SymbolKind::Constant, SymbolKind::Variable]);
}

#[test]
fn a_definition_identifier_is_not_also_a_reference() {
    let src = "def alpha():\n    pass\n\n\ndef beta():\n    alpha()\n";
    let f = facts(src);
    let alpha_refs: Vec<_> = f.references.iter().filter(|r| r.name == "alpha").collect();
    assert_eq!(alpha_refs.len(), 1, "got {alpha_refs:?}");
    assert_eq!(alpha_refs[0].kind, ReferenceKind::Call);
    assert!(alpha_refs[0].span.start > src.find("def beta").unwrap());
}

#[test]
fn references_start_unresolved() {
    let f = facts("def f():\n    g()\n");
    let r = f.references.iter().find(|r| r.name == "g").unwrap();
    assert_eq!(r.target, None);
    assert_eq!(r.confidence, Confidence::NameOnly);
}

#[test]
fn plain_and_dotted_imports_capture_their_path() {
    let f = facts("import os\nimport os.path\nimport a, b\n");
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["os", "os.path", "a", "b"]);
    assert!(f.imports.iter().all(|i| i.alias.is_none()));
}

#[test]
fn aliased_module_imports_capture_the_alias() {
    let f = facts("import os.path as p\n");
    assert_eq!(f.imports.len(), 1);
    assert_eq!(f.imports[0].path, "os.path");
    assert_eq!(f.imports[0].alias.as_deref(), Some("p"));
}

#[test]
fn from_imports_capture_one_import_per_name() {
    // Tree-sitter reports one match per imported name, so a statement importing
    // several names produces several Imports that share a path.
    let f = facts("from m import a, b as c\n");
    assert_eq!(f.imports.len(), 2, "got {:?}", f.imports);
    assert!(f.imports.iter().all(|i| i.path == "m"));

    let plain = f
        .imports
        .iter()
        .find(|i| i.names.iter().any(|n| n.local == "a"))
        .expect("plain name");
    assert_eq!(plain.names[0].original, "a");
    assert!(!plain.names[0].is_aliased());

    let aliased = f
        .imports
        .iter()
        .find(|i| i.names.iter().any(|n| n.local == "c"))
        .expect("aliased name");
    assert_eq!(aliased.names[0].original, "b");
    assert!(aliased.names[0].is_aliased());
}

#[test]
fn star_imports_are_marked_as_globs() {
    let f = facts("from m import *\n");
    assert_eq!(f.imports.len(), 1);
    assert!(f.imports[0].is_glob);
    assert_eq!(f.imports[0].path, "m");
}

#[test]
fn relative_imports_keep_their_leading_dots() {
    let f = facts("from . import sibling\nfrom .sub import thing\nfrom ..pkg import other\n");
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec![".", ".sub", "..pkg"]);
    let first = &f.imports[0];
    assert_eq!(first.names[0].local, "sibling");
}

#[test]
fn future_imports_are_captured() {
    let f = facts("from __future__ import annotations\n");
    assert_eq!(f.imports.len(), 1);
    assert_eq!(f.imports[0].path, "__future__");
    assert_eq!(f.imports[0].names[0].local, "annotations");
}

#[test]
fn imported_names_are_not_definitions_but_are_references() {
    // An import binds a local name, but the binding lives in FileFacts::imports, not in
    // symbols.
    let src = "from m import thing\n";
    let f = facts(src);
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
    let r = f.references.iter().find(|r| r.name == "thing").unwrap();
    assert_eq!(r.span.text(src), "thing");
}

#[test]
fn function_and_class_bodies_are_scopes_but_blocks_are_not() {
    // Python scoping: `def`, `class`, `lambda` and comprehensions introduce
    // scopes; an `if` block does not.
    let src = "def outer():\n    x = 1\n    if x:\n        y = 2\n    return y\n";
    let f = facts(src);
    let outer_scope = f.scope_at(src.find("x = 1").unwrap()).unwrap();
    let if_body_scope = f.scope_at(src.find("y = 2").unwrap()).unwrap();
    assert_eq!(
        outer_scope, if_body_scope,
        "an if block must not introduce a scope"
    );

    let module_scope = f.scope_at(src.find("def outer").unwrap()).unwrap();
    assert_ne!(module_scope, outer_scope);
    assert!(f.scope_chain(outer_scope).contains(&module_scope));
}

#[test]
fn comprehensions_and_lambdas_introduce_scopes() {
    let src = "def f(xs):\n    a = [i for i in xs]\n    g = lambda z: z\n    return a, g\n";
    let f = facts(src);
    let fn_scope = f.scope_at(src.find("a = [").unwrap()).unwrap();
    let comp_scope = f.scope_at(src.find("for i in xs").unwrap()).unwrap();
    assert_ne!(fn_scope, comp_scope);
    assert!(f.scope_chain(comp_scope).contains(&fn_scope));

    let lambda_scope = f.scope_at(src.find("lambda z").unwrap()).unwrap();
    assert_ne!(fn_scope, lambda_scope);
}

#[test]
fn symbol_and_reference_lookup_by_offset() {
    let src = "def alpha():\n    pass\n\n\ndef beta():\n    alpha()\n";
    let f = facts(src);
    let def_offset = src.find("alpha").unwrap() + 1;
    assert_eq!(
        f.symbol_at(def_offset).map(|s| s.name.as_str()),
        Some("alpha")
    );
    let call_offset = src.rfind("alpha").unwrap() + 1;
    assert_eq!(
        f.reference_at(call_offset).map(|r| r.name.as_str()),
        Some("alpha")
    );
}

#[test]
fn the_rich_sample_yields_every_definition_kind() {
    let f = facts(RICH);
    for (kind, expected) in [
        (SymbolKind::Function, "free"),
        (SymbolKind::Method, "render"),
        (SymbolKind::Class, "Widget"),
        (SymbolKind::TypeAlias, "Alias"),
        (SymbolKind::Constant, "CONST"),
        (SymbolKind::Variable, "value"),
        (SymbolKind::Parameter, "target"),
        (SymbolKind::Field, "kind"),
    ] {
        assert!(
            names_of(&f, kind).contains(&expected),
            "{kind:?} should include {expected}, got {:?}",
            names_of(&f, kind)
        );
    }
}
