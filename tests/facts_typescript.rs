//! Fact-extraction tests for `queries/typescript/facts.scm`.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers, span::Span};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "sample does not parse cleanly as {lang}"
    );
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

/// Both grammars, for constructs they share.
fn both(src: &str) -> [(Language, FileFacts); 2] {
    [
        (Language::TypeScript, facts(Language::TypeScript, src)),
        (Language::Tsx, facts(Language::Tsx, src)),
    ]
}

fn named<'a>(f: &'a FileFacts, name: &str) -> Vec<&'a Symbol> {
    f.symbols.iter().filter(|s| s.name == name).collect()
}

/// The single symbol called `name`; fails loudly if the query produced none or
/// several, which is how duplicate definitions would surface.
fn one<'a>(f: &'a FileFacts, name: &str) -> &'a Symbol {
    let all = named(f, name);
    assert_eq!(
        all.len(),
        1,
        "expected exactly one symbol named {name:?}, got {:?}",
        all.iter()
            .map(|s| (s.kind, s.full_span))
            .collect::<Vec<_>>()
    );
    all[0]
}

fn refs_named<'a>(f: &'a FileFacts, name: &str) -> Vec<&'a Reference> {
    f.references.iter().filter(|r| r.name == name).collect()
}

const DECLS: &str = r#"export function exportedFn(a: number): void {}
function plainFn() {}
export const arrowFn = (x: string) => x;
const exprFn = function () { return 1; };
export default class Defaulted {}
export class Klass {}
abstract class Abstract {}
export interface Iface { m(): void; p: string; }
export type Alias = { k: string };
export enum Colour { Red = 1, Blue }
export const CONST_X = 1;
const constY = 2;
let mutable = 3;
var older = 4;
let uninit: Iface;
export namespace Space { export const inside = 1; }
namespace Plain { const hidden = 5; }
"#;

const MEMBERS: &str = r#"class Widget {
  private count: number = 1;
  static LABEL = "w";
  bare;
  handler = () => {};
  constructor(private dep: string) {}
  get value(): number { return this.count; }
  set value(v: number) { this.count = v; }
  render(mode: string) { return mode; }
  #secret() { return 1; }
}
interface Shape { area(): number; sides: number; }
"#;

const PARAMS: &str = r#"function all(a: number, b?: string, c = 1, { d, e: renamed, ...restProps }: P, [f, ...g]: Q, ...rest: R[]) {}
const single = x => x;
"#;

const IMPORTS: &str = r#"import def from './default-mod';
import { alpha, beta as gamma } from './named-mod';
import * as ns from './ns-mod';
import type { Only } from './type-mod';
import './side-effect';
const cjs = require('./cjs-mod');
export { alpha, gamma as delta };
export * from './re-export';
"#;

const USES: &str = r#"import { Helper } from './h';
function caller(input: Helper) {
  plainTarget();
  receiver.methodTarget(1);
  const made = new Klass();
  const cast = input as Alias;
  return receiver.fieldTarget;
}
"#;

const JSX: &str = r#"import { Helper } from './helper';

export function Card({ title }: Props) {
  return (
    <div className="card wide" id="main">
      <span class="label">{title}</span>
      <Helper onSelect={title} />
      <ns.Widget />
    </div>
  );
}

const Small = () => <p className="tiny">hi</p>;
"#;

#[test]
fn samples_parse_cleanly_in_both_grammars() {
    // `facts` asserts this; the JSX sample is tsx-only by construction.
    for src in [DECLS, MEMBERS, PARAMS, IMPORTS, USES] {
        both(src);
    }
    facts(Language::Tsx, JSX);
}

#[test]
fn every_declaration_kind_is_found() {
    for (lang, f) in both(DECLS) {
        let expect = [
            ("exportedFn", SymbolKind::Function),
            ("plainFn", SymbolKind::Function),
            ("arrowFn", SymbolKind::Function),
            ("exprFn", SymbolKind::Function),
            ("Defaulted", SymbolKind::Class),
            ("Klass", SymbolKind::Class),
            ("Abstract", SymbolKind::Class),
            ("Iface", SymbolKind::Interface),
            ("Alias", SymbolKind::TypeAlias),
            ("Colour", SymbolKind::Enum),
            ("CONST_X", SymbolKind::Constant),
            ("constY", SymbolKind::Constant),
            ("mutable", SymbolKind::Variable),
            ("older", SymbolKind::Variable),
            ("uninit", SymbolKind::Variable),
            ("Space", SymbolKind::Module),
            ("Plain", SymbolKind::Module),
        ];
        for (name, kind) in expect {
            assert_eq!(one(&f, name).kind, kind, "{lang} symbol {name}");
        }
    }
}

#[test]
fn name_span_covers_only_the_identifier() {
    for (lang, f) in both(DECLS) {
        for name in [
            "exportedFn",
            "arrowFn",
            "Klass",
            "Iface",
            "Alias",
            "Colour",
            "Space",
        ] {
            let s = one(&f, name);
            assert_eq!(s.name_span.text(DECLS), name, "{lang} name span for {name}");
            assert!(s.full_span.contains(s.name_span), "{lang} {name}");
        }
    }
}

#[test]
fn full_span_of_an_exported_declaration_excludes_the_export_keyword() {
    // The `export` wrapper is a separate statement node; the symbol is the
    // declaration itself, which a move or delete rewrites.
    let f = facts(Language::TypeScript, DECLS);
    assert_eq!(
        one(&f, "plainFn").full_span.text(DECLS),
        "function plainFn() {}"
    );
    assert_eq!(
        one(&f, "exportedFn").full_span.text(DECLS),
        "function exportedFn(a: number): void {}"
    );
}

#[test]
fn export_visibility_is_recorded() {
    for (lang, f) in both(DECLS) {
        for name in [
            "exportedFn",
            "arrowFn",
            "Defaulted",
            "Klass",
            "Iface",
            "Alias",
            "Colour",
            "CONST_X",
            "Space",
        ] {
            assert!(one(&f, name).exported, "{lang}: {name} should be exported");
        }
        for name in [
            "plainFn", "exprFn", "Abstract", "constY", "mutable", "older", "Plain",
        ] {
            assert!(!one(&f, name).exported, "{lang}: {name} is not exported");
        }
    }
}

#[test]
fn exported_declarations_do_not_produce_duplicate_symbols() {
    // The export pattern and the plain pattern must be mutually exclusive: the
    // extractor deduplicates references but not definitions.
    for (lang, f) in both(DECLS) {
        let mut seen: Vec<(&str, Span)> = f
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.name_span))
            .collect();
        let before = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(
            before,
            seen.len(),
            "{lang}: duplicate definitions in {seen:?}"
        );
    }
}

#[test]
fn const_is_a_constant_and_let_var_are_variables() {
    // Documented rule: the declaring keyword decides, not the name's casing.
    let f = facts(Language::TypeScript, DECLS);
    assert_eq!(one(&f, "CONST_X").kind, SymbolKind::Constant);
    assert_eq!(one(&f, "constY").kind, SymbolKind::Constant);
    assert_eq!(one(&f, "mutable").kind, SymbolKind::Variable);
    assert_eq!(one(&f, "older").kind, SymbolKind::Variable);
}

#[test]
fn a_function_valued_binding_is_a_function_not_a_variable() {
    for (lang, f) in both(DECLS) {
        for name in ["arrowFn", "exprFn"] {
            let s = one(&f, name);
            assert_eq!(s.kind, SymbolKind::Function, "{lang}: {name}");
            // Only the binding identifier is renameable.
            assert_eq!(s.name_span.text(DECLS), name);
        }
    }
}

#[test]
fn destructured_bindings_become_one_symbol_per_name() {
    let src = "const { alpha, beta: renamed, ...others } = source;\nlet [first, ...tail] = list;\n";
    for (lang, f) in both(src) {
        for name in ["alpha", "renamed", "others", "first", "tail"] {
            let s = one(&f, name);
            assert_eq!(s.kind, SymbolKind::Variable, "{lang}: {name}");
            assert_eq!(s.name_span.text(src), name);
        }
        // Documented limit: the `beta` half of `beta: renamed` names a property of
        // the right-hand side, not a binding, so it is not a symbol.
        assert!(named(&f, "beta").is_empty(), "{lang}");
    }
}

#[test]
fn namespace_members_keep_their_own_visibility() {
    let f = facts(Language::TypeScript, DECLS);
    assert!(one(&f, "inside").exported);
    assert!(!one(&f, "hidden").exported);
}

#[test]
fn class_members_are_qualified_methods_and_fields() {
    for (lang, f) in both(MEMBERS) {
        let render = one(&f, "render");
        assert_eq!(render.kind, SymbolKind::Method, "{lang}");
        assert_eq!(render.qualifier.as_deref(), Some("Widget"));
        assert_eq!(render.qualified_name(), "Widget::render");

        assert_eq!(one(&f, "constructor").kind, SymbolKind::Method, "{lang}");
        assert_eq!(one(&f, "#secret").kind, SymbolKind::Method, "{lang}");

        for name in ["count", "LABEL", "bare"] {
            let s = one(&f, name);
            assert_eq!(s.kind, SymbolKind::Field, "{lang}: {name}");
            assert_eq!(s.qualifier.as_deref(), Some("Widget"));
        }

        // A class field holding an arrow is a method.
        let handler = one(&f, "handler");
        assert_eq!(handler.kind, SymbolKind::Method, "{lang}");
        assert_eq!(handler.qualifier.as_deref(), Some("Widget"));
    }
}

#[test]
fn the_class_itself_stays_a_single_renameable_symbol() {
    // `Widget` is both a definition and a container; the container pattern must
    // not add a second definition of the same name.
    for (lang, f) in both(MEMBERS) {
        let w = one(&f, "Widget");
        assert_eq!(w.kind, SymbolKind::Class, "{lang}");
        assert_eq!(w.qualifier, None, "{lang}");
    }
}

#[test]
fn a_getter_and_setter_pair_are_two_declarations() {
    // Documented limit: the query language cannot tell `get value` from
    // `set value`, so both stay separate symbols with distinct spans.
    let f = facts(Language::TypeScript, MEMBERS);
    let vals = named(&f, "value");
    assert_eq!(vals.len(), 2, "got {vals:?}");
    assert_ne!(vals[0].full_span, vals[1].full_span);
    assert!(vals.iter().all(|s| s.kind == SymbolKind::Method));
}

#[test]
fn interface_members_are_qualified() {
    for (lang, f) in both(MEMBERS) {
        let area = one(&f, "area");
        assert_eq!(area.kind, SymbolKind::Method, "{lang}");
        assert_eq!(area.qualified_name(), "Shape::area");

        let sides = one(&f, "sides");
        assert_eq!(sides.kind, SymbolKind::Field, "{lang}");
        assert_eq!(sides.qualifier.as_deref(), Some("Shape"));
    }
}

#[test]
fn enum_members_are_fields_qualified_by_the_enum() {
    let src = "enum Colour { Red = 1, Blue }\n";
    for (lang, f) in both(src) {
        for name in ["Red", "Blue"] {
            let s = one(&f, name);
            assert_eq!(s.kind, SymbolKind::Field, "{lang}: {name}");
            assert_eq!(s.qualifier.as_deref(), Some("Colour"), "{lang}: {name}");
            assert_eq!(s.name_span.text(src), name);
        }
    }
}

#[test]
fn parameters_of_every_shape_are_captured() {
    for (lang, f) in both(PARAMS) {
        for name in ["a", "b", "c", "d", "renamed", "restProps", "f", "g", "rest"] {
            let s = one(&f, name);
            assert_eq!(s.kind, SymbolKind::Parameter, "{lang}: {name}");
            assert_eq!(s.name_span.text(PARAMS), name, "{lang}: {name}");
        }
        // A parenthesis-less arrow parameter is the identifier itself.
        let x = one(&f, "x");
        assert_eq!(x.kind, SymbolKind::Parameter, "{lang}");
        assert_eq!(x.full_span, x.name_span, "{lang}");
    }
}

#[test]
fn a_typed_parameters_span_stops_at_the_identifier() {
    let f = facts(Language::TypeScript, PARAMS);
    let a = one(&f, "a");
    assert_eq!(a.name_span.text(PARAMS), "a");
    assert_eq!(a.full_span.text(PARAMS), "a: number");
    assert_eq!(one(&f, "b").full_span.text(PARAMS), "b?: string");
    assert_eq!(one(&f, "c").full_span.text(PARAMS), "c = 1");
}

#[test]
fn loop_and_catch_bindings_are_variables() {
    let src = "for (const it of list) { use(it); }\ntry { risky(); } catch (err) { log(err); }\nfor (let i = 0; i < 3; i++) {}\n";
    for (lang, f) in both(src) {
        for name in ["it", "err", "i"] {
            assert_eq!(one(&f, name).kind, SymbolKind::Variable, "{lang}: {name}");
        }
    }
}

#[test]
fn every_import_form_is_recorded() {
    for (lang, f) in both(IMPORTS) {
        let by_path =
            |p: &str| -> Vec<&Import> { f.imports.iter().filter(|i| i.path == p).collect() };

        // Default import: one binding, unaliased.
        let d = by_path("./default-mod");
        assert_eq!(d.len(), 1, "{lang}: {d:?}");
        assert_eq!(d[0].names[0].local, "def");
        assert!(!d[0].names[0].is_aliased());

        // Named imports: one record per bound name so aliases pair correctly.
        let n = by_path("./named-mod");
        let mut bound: Vec<(&str, &str)> = n
            .iter()
            .flat_map(|i| i.names.iter())
            .map(|x| (x.local.as_str(), x.original.as_str()))
            .collect();
        bound.sort();
        assert_eq!(bound, vec![("alpha", "alpha"), ("gamma", "beta")], "{lang}");

        // Namespace import is a glob and binds an alias.
        let g = by_path("./ns-mod");
        assert_eq!(g.len(), 1, "{lang}");
        assert!(g[0].is_glob, "{lang}");
        assert_eq!(g[0].alias.as_deref(), Some("ns"), "{lang}");

        // `import type { … }` is an ordinary named import to the grammar.
        assert_eq!(by_path("./type-mod")[0].names[0].local, "Only", "{lang}");

        // Side-effect import binds nothing.
        let s = by_path("./side-effect");
        assert_eq!(s.len(), 1, "{lang}");
        assert!(s[0].names.is_empty(), "{lang}");

        // CommonJS and re-exports are module dependencies too.
        assert_eq!(by_path("./cjs-mod").len(), 1, "{lang}");
        assert_eq!(by_path("./re-export").len(), 1, "{lang}");
    }
}

#[test]
fn import_paths_are_unquoted() {
    let f = facts(Language::TypeScript, IMPORTS);
    assert!(
        f.imports.iter().all(|i| !i.path.contains(['"', '\''])),
        "{:?}",
        f.imports
    );
}

#[test]
fn a_side_effect_import_does_not_also_match_the_named_forms() {
    let f = facts(Language::TypeScript, "import './only';\n");
    assert_eq!(f.imports.len(), 1, "{:?}", f.imports);
}

#[test]
fn export_clauses_are_references_not_definitions() {
    for (lang, f) in both(IMPORTS) {
        // `export { alpha, gamma as delta }` re-exports existing bindings.
        assert!(named(&f, "delta").is_empty(), "{lang}");
        assert!(named(&f, "alpha").is_empty(), "{lang}");
        assert!(!refs_named(&f, "alpha").is_empty(), "{lang}");
        assert!(!refs_named(&f, "gamma").is_empty(), "{lang}");
    }
}

#[test]
fn calls_are_recorded_as_calls() {
    for (lang, f) in both(USES) {
        let plain = refs_named(&f, "plainTarget");
        assert_eq!(plain.len(), 1, "{lang}: {plain:?}");
        assert_eq!(plain[0].kind, ReferenceKind::Call, "{lang}");

        // `obj.m()` records the property, which is the renameable half.
        let method = refs_named(&f, "methodTarget");
        assert_eq!(method[0].kind, ReferenceKind::Call, "{lang}");

        // Documented choice: `new X()` is a call.
        let ctor = refs_named(&f, "Klass");
        assert_eq!(ctor[0].kind, ReferenceKind::Call, "{lang}");
    }
}

#[test]
fn member_access_is_a_field_reference() {
    for (lang, f) in both(USES) {
        let field = refs_named(&f, "fieldTarget");
        assert_eq!(field.len(), 1, "{lang}: {field:?}");
        assert_eq!(field[0].kind, ReferenceKind::Field, "{lang}");
        assert_eq!(
            refs_named(&f, "receiver")[0].kind,
            ReferenceKind::Identifier,
            "{lang}"
        );
    }
}

#[test]
fn type_positions_are_type_references() {
    for (lang, f) in both(USES) {
        // Parameter annotation and `as` cast.
        assert_eq!(
            refs_named(&f, "Helper")
                .iter()
                .find(|r| r.span.start > USES.find("function caller").unwrap())
                .unwrap()
                .kind,
            ReferenceKind::Type,
            "{lang}"
        );
        assert_eq!(
            refs_named(&f, "Alias")[0].kind,
            ReferenceKind::Type,
            "{lang}"
        );
    }
}

#[test]
fn heritage_clauses_are_type_references() {
    let src = "class Sub extends Sup implements Contract {}\n";
    for (lang, f) in both(src) {
        assert_eq!(refs_named(&f, "Sup")[0].kind, ReferenceKind::Type, "{lang}");
        assert_eq!(
            refs_named(&f, "Contract")[0].kind,
            ReferenceKind::Type,
            "{lang}"
        );
        // The subclass name is a definition, so it is not also a reference.
        assert!(refs_named(&f, "Sub").is_empty(), "{lang}");
    }
}

#[test]
fn definition_identifiers_are_not_also_references() {
    let src = "function target() {}\nfunction other() { target(); }\n";
    for (lang, f) in both(src) {
        let hits = refs_named(&f, "target");
        assert_eq!(hits.len(), 1, "{lang}: {hits:?}");
        assert_eq!(hits[0].kind, ReferenceKind::Call, "{lang}");
        assert!(hits[0].span.start > src.find("function other").unwrap());
    }
}

#[test]
fn references_start_unresolved() {
    let f = facts(Language::TypeScript, USES);
    assert!(f
        .references
        .iter()
        .all(|r| r.target.is_none() && r.confidence == Confidence::NameOnly));
}

#[test]
fn scopes_nest_by_containment() {
    let src = "function outer() {\n  const a = 1;\n  {\n    const b = 2;\n  }\n}\n";
    for (lang, f) in both(src) {
        let inner = f.scope_at(src.find("const b").unwrap()).unwrap();
        let outer = f.scope_at(src.find("const a").unwrap()).unwrap();
        assert_ne!(inner, outer, "{lang}");
        assert!(f.scope_chain(inner).contains(&outer), "{lang}");
    }
}

#[test]
fn an_arrow_body_is_its_own_scope() {
    let src = "const f = () => { const inside = 1; return inside; };\nconst top = 2;\n";
    for (lang, f) in both(src) {
        let inner = f.scope_at(src.find("const inside").unwrap()).unwrap();
        let outer = f.scope_at(src.find("const top").unwrap()).unwrap();
        assert_ne!(inner, outer, "{lang}");
    }
}

#[test]
fn jsx_component_names_are_references_and_html_tags_are_not() {
    let f = facts(Language::Tsx, JSX);

    // Capitalised element names reference a component symbol.
    let helper: Vec<_> = refs_named(&f, "Helper")
        .into_iter()
        .filter(|r| r.span.start > JSX.find("return").unwrap())
        .collect();
    assert_eq!(helper.len(), 1, "{helper:?}");
    assert_eq!(helper[0].kind, ReferenceKind::Identifier);

    // Lowercase tags are HTML, not symbols, the identifier catch-all is scoped
    // to expression positions precisely so these never appear.
    for tag in ["div", "span", "p"] {
        assert!(
            refs_named(&f, tag).is_empty(),
            "{tag} should not be a reference: {:?}",
            refs_named(&f, tag)
        );
    }
}

#[test]
fn a_dotted_jsx_component_is_a_field_reference() {
    // `<ns.Widget />` spells its name as a member expression.
    let f = facts(Language::Tsx, JSX);
    let w = refs_named(&f, "Widget");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].kind, ReferenceKind::Field);
}

#[test]
fn class_name_attributes_become_string_references() {
    let f = facts(Language::Tsx, JSX);
    let names: Vec<&str> = f
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::StringRef)
        .map(|r| r.name.as_str())
        .collect();

    // `className`, `class` and `id` all count; a multi-class value fans out into
    // one reference per class name.
    for want in ["card", "wide", "main", "label", "tiny"] {
        assert!(names.contains(&want), "missing {want} in {names:?}");
    }
    // The quotes are not part of the reference.
    assert!(names.iter().all(|n| !n.contains('"')), "{names:?}");
    // Attribute names themselves are not references.
    assert!(refs_named(&f, "className").is_empty());
}

#[test]
fn a_jsx_returning_arrow_is_still_a_function() {
    let f = facts(Language::Tsx, JSX);
    assert_eq!(one(&f, "Small").kind, SymbolKind::Function);
    let card = one(&f, "Card");
    assert_eq!(card.kind, SymbolKind::Function);
    assert!(card.exported);
    assert_eq!(one(&f, "title").kind, SymbolKind::Parameter);
}

#[test]
fn a_realistic_component_file_extracts_without_duplicate_definitions() {
    let src = r#"import React, { useState, useCallback } from 'react';
import type { User } from './types';
import * as api from './api';

export interface Props {
  user: User;
  onSave?: (u: User) => void;
}

const DEFAULT_NAME = 'anonymous';

export default function Profile({ user, onSave }: Props) {
  const [name, setName] = useState(user.name ?? DEFAULT_NAME);
  const save = useCallback(() => {
    api.persist({ ...user, name });
    onSave?.(user);
  }, [user, name]);

  return (
    <form className="profile form" onSubmit={save}>
      <label className="field" htmlFor="name">Name</label>
      <input id="name" value={name} onChange={e => setName(e.target.value)} />
      <Actions onSave={save} />
    </form>
  );
}

class Store<T> {
  private items: T[] = [];
  add(item: T): void { this.items.push(item); }
  get size(): number { return this.items.length; }
}

export const store = new Store<User>();
"#;
    let f = facts(Language::Tsx, src);
    let mut spans: Vec<Span> = f.symbols.iter().map(|s| s.name_span).collect();
    let before = spans.len();
    spans.sort();
    spans.dedup();
    assert_eq!(
        before,
        spans.len(),
        "duplicate definitions in {:?}",
        f.symbols
    );

    assert!(one(&f, "Profile").exported);
    assert_eq!(one(&f, "add").qualified_name(), "Store::add");
    assert_eq!(one(&f, "DEFAULT_NAME").kind, SymbolKind::Constant);
    // A binding whose initialiser is a *call*, `useCallback(() => …)` and every other
    // higher-order wrapper, holds a value, not a literal function, so it is a Constant.
    assert_eq!(one(&f, "save").kind, SymbolKind::Constant);
    let css: Vec<&str> = f
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::StringRef)
        .map(|r| r.name.as_str())
        .collect();
    assert!(
        css.contains(&"profile") && css.contains(&"field"),
        "{css:?}"
    );
}

#[test]
fn the_two_grammars_agree_on_shared_syntax() {
    // The whole point of one query file: identical facts from both grammars.
    let [(_, ts), (_, tsx)] = both(DECLS);
    let key = |f: &FileFacts| -> Vec<(String, SymbolKind, Span, bool)> {
        let mut v: Vec<_> = f
            .symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind, s.name_span, s.exported))
            .collect();
        v.sort();
        v
    };
    assert_eq!(key(&ts), key(&tsx));
}

#[test]
fn export_declare_is_captured_but_not_marked_exported() {
    // Known limit: `declare` inserts an ambient_declaration between the export
    // statement and the declaration, so the export pattern cannot reach it.
    let src = "export declare function ambient(): void;\ndeclare function local(): void;\n";
    let f = facts(Language::TypeScript, src);
    assert_eq!(one(&f, "ambient").kind, SymbolKind::Function);
    assert!(!one(&f, "ambient").exported);
    assert_eq!(one(&f, "local").kind, SymbolKind::Function);
}

#[test]
fn a_declaration_in_an_exotic_statement_position_is_not_captured() {
    // Known limit: definitions are matched through their statement parent (`program` /
    // `statement_block` / `export_statement`).
    let src =
        "switch (k) { case 1: { const inBlock = 1; } }\nswitch (k) { case 2: const inCase = 2; }\n";
    let f = facts(Language::TypeScript, src);
    assert_eq!(named(&f, "inBlock").len(), 1);
    assert!(named(&f, "inCase").is_empty());
}

#[test]
fn a_typeof_type_query_names_the_value_it_reads() {
    // `typeof Foo` reads a *value's* type.
    let src = "const Foo = { a: 1 };\nlet x: typeof Foo;\n";
    let f = facts(Language::TypeScript, src);
    let uses: Vec<_> = f.references.iter().filter(|r| r.name == "Foo").collect();
    assert_eq!(uses.len(), 1, "got {uses:?}");
    assert_eq!(uses[0].kind, ReferenceKind::Type);
}
