; TypeScript / TSX fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; One file serves both grammars — LANGUAGE_TYPESCRIPT (.ts) and LANGUAGE_TSX
; (.tsx). They share every node type except JSX: `jsx_element`, `jsx_attribute`,
; `jsx_opening_element` and friends exist only in the tsx grammar, and naming a
; node type the grammar does not define makes `Query::new` fail. So the JSX
; rules at the end of this file are written structurally — through the `name:`
; field and through attribute shape — and not by naming JSX nodes.

; ---------------------------------------------------------------- scopes
;
; Parameters sit outside the body block, so they share the enclosing scope with
; the function's own name (same as the Rust queries). Arrow functions are the
; exception: the whole arrow is the scope, because an expression-bodied arrow
; has no statement block to stand in for one.
(program) @scope
(statement_block) @scope
(class_body) @scope
(arrow_function) @scope
(for_statement) @scope
(for_in_statement) @scope
(catch_clause) @scope
(switch_body) @scope

; ------------------------------------------------------------- containers
;
; A container qualifies the symbols nested inside it: a function declared in a
; class body becomes `Class::method`. Containers are separate patterns from the
; definitions of the same nodes, so `class C` stays a single renameable symbol.
(class_declaration
  name: (type_identifier) @container.name) @container

(abstract_class_declaration
  name: (type_identifier) @container.name) @container

(interface_declaration
  name: (type_identifier) @container.name) @container

(type_alias_declaration
  name: (type_identifier) @container.name) @container

(enum_declaration
  name: (identifier) @container.name) @container

; ------------------------------------------------ top-level declarations
;
; `export function f() {}` parses as an export_statement *wrapping* a
; function_declaration, and captures only group within a single pattern match,
; so @export has to be captured in the same pattern as the definition it marks.
; That needs one pattern for the exported form and one for the plain form — and
; the plain form must not also match inside an export_statement, or every
; exported declaration would yield two identical symbols (the extractor does not
; deduplicate definitions).
;
; tree-sitter cannot say "my parent is not an export_statement": a negated field
; (`!declaration`) is silently ignored when the pattern root is the `_` wildcard.
; So unexported declarations are matched through their concrete statement
; parents instead — `program` and `statement_block`, which is where declarations
; legally appear. A declaration in an exotic statement position (a `switch` case,
; a braceless `else`) is therefore not captured.

(program [
  ; -- named functions and types
  (function_declaration
    name: (identifier) @name) @definition.function
  (generator_function_declaration
    name: (identifier) @name) @definition.function
  (class_declaration
    name: (type_identifier) @name) @definition.class
  (abstract_class_declaration
    name: (type_identifier) @name) @definition.class
  (interface_declaration
    name: (type_identifier) @name) @definition.interface
  (type_alias_declaration
    name: (type_identifier) @name) @definition.type
  (enum_declaration
    name: (identifier) @name) @definition.enum

  ; -- a function value bound to a name is a function declaration in all but
  ;    spelling, and is how most modern TypeScript declares functions. The
  ;    wildcard parent here is the const/let/var declaration itself.
  (_ (variable_declarator
       name: (identifier) @name
       value: [(arrow_function) (function_expression) (generator_function)])
     @definition.function)
  (_ (variable_declarator
       name: (identifier) @name
       value: (class))
     @definition.class)

  ; -- plain bindings. `const` is a Constant, `let` and `var` are Variables.
  ;    `value: (_ !body)` excludes exactly the function- and class-valued
  ;    declarators handled above: those are the only initialisers with a `body`
  ;    field, so the two rules partition the declarators between them.
  (lexical_declaration kind: "const"
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.constant)
  (lexical_declaration kind: "const"
    (variable_declarator name: (identifier) @name !value)
    @definition.constant)
  (lexical_declaration kind: "let"
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.variable)
  (lexical_declaration kind: "let"
    (variable_declarator name: (identifier) @name !value)
    @definition.variable)
  (variable_declaration
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.variable)
  (variable_declaration
    (variable_declarator name: (identifier) @name !value)
    @definition.variable)

  ; -- destructuring binds several names to one declarator; each name is its own
  ;    match, so each becomes its own symbol sharing the declarator's full span.
  ;    Destructured bindings are Variables whatever keyword introduced them.
  (_ (variable_declarator
       name: (object_pattern [
         (shorthand_property_identifier_pattern) @name
         (pair_pattern value: (identifier) @name)
         (rest_pattern (identifier) @name)]))
     @definition.variable)
  (_ (variable_declarator
       name: (array_pattern [
         (identifier) @name
         (rest_pattern (identifier) @name)]))
     @definition.variable)
])

(statement_block [
  (function_declaration
    name: (identifier) @name) @definition.function
  (generator_function_declaration
    name: (identifier) @name) @definition.function
  (class_declaration
    name: (type_identifier) @name) @definition.class
  (abstract_class_declaration
    name: (type_identifier) @name) @definition.class
  (interface_declaration
    name: (type_identifier) @name) @definition.interface
  (type_alias_declaration
    name: (type_identifier) @name) @definition.type
  (enum_declaration
    name: (identifier) @name) @definition.enum

  (_ (variable_declarator
       name: (identifier) @name
       value: [(arrow_function) (function_expression) (generator_function)])
     @definition.function)
  (_ (variable_declarator
       name: (identifier) @name
       value: (class))
     @definition.class)

  (lexical_declaration kind: "const"
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.constant)
  (lexical_declaration kind: "const"
    (variable_declarator name: (identifier) @name !value)
    @definition.constant)
  (lexical_declaration kind: "let"
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.variable)
  (lexical_declaration kind: "let"
    (variable_declarator name: (identifier) @name !value)
    @definition.variable)
  (variable_declaration
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.variable)
  (variable_declaration
    (variable_declarator name: (identifier) @name !value)
    @definition.variable)

  (_ (variable_declarator
       name: (object_pattern [
         (shorthand_property_identifier_pattern) @name
         (pair_pattern value: (identifier) @name)
         (rest_pattern (identifier) @name)]))
     @definition.variable)
  (_ (variable_declarator
       name: (array_pattern [
         (identifier) @name
         (rest_pattern (identifier) @name)]))
     @definition.variable)
])

; The same set again, wrapped in `export` — this is the only place @export can
; be captured alongside the definition it marks. `export default class D {}` and
; `export default function f() {}` land here too.
(export_statement [
  (function_declaration
    name: (identifier) @name) @definition.function
  (generator_function_declaration
    name: (identifier) @name) @definition.function
  (class_declaration
    name: (type_identifier) @name) @definition.class
  (abstract_class_declaration
    name: (type_identifier) @name) @definition.class
  (interface_declaration
    name: (type_identifier) @name) @definition.interface
  (type_alias_declaration
    name: (type_identifier) @name) @definition.type
  (enum_declaration
    name: (identifier) @name) @definition.enum

  (_ (variable_declarator
       name: (identifier) @name
       value: [(arrow_function) (function_expression) (generator_function)])
     @definition.function)
  (_ (variable_declarator
       name: (identifier) @name
       value: (class))
     @definition.class)

  (lexical_declaration kind: "const"
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.constant)
  (lexical_declaration kind: "const"
    (variable_declarator name: (identifier) @name !value)
    @definition.constant)
  (lexical_declaration kind: "let"
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.variable)
  (lexical_declaration kind: "let"
    (variable_declarator name: (identifier) @name !value)
    @definition.variable)
  (variable_declaration
    (variable_declarator name: (identifier) @name value: (_ !body))
    @definition.variable)
  (variable_declaration
    (variable_declarator name: (identifier) @name !value)
    @definition.variable)

  (_ (variable_declarator
       name: (object_pattern [
         (shorthand_property_identifier_pattern) @name
         (pair_pattern value: (identifier) @name)
         (rest_pattern (identifier) @name)]))
     @definition.variable)
  (_ (variable_declarator
       name: (array_pattern [
         (identifier) @name
         (rest_pattern (identifier) @name)]))
     @definition.variable)
]) @export

; ------------------------------------------------------ namespaces, ambient
;
; A bare `namespace NS {}` is an expression statement, not a declaration, so it
; needs its own parent. `declare`d declarations sit under an ambient_declaration,
; which hides them from the export patterns above: `export declare function f()`
; is captured but not marked exported.
(expression_statement
  (internal_module
    name: (identifier) @name) @definition.module)

(export_statement
  (internal_module
    name: (identifier) @name) @definition.module) @export

(ambient_declaration
  (module
    name: (identifier) @name) @definition.module)

; A single wildcard parent is safe wherever no @export variant exists: a node has
; exactly one parent, so the pattern can match only once.
(_ (function_signature
     name: (identifier) @name) @definition.function)

; ------------------------------------------------------------- parameters
;
; Destructured parameters bind each element separately, sharing the parameter's
; full span. Property renames inside a destructuring pattern (`{a: local}`) bind
; `local`; the `a` half names a field of the argument and is not captured here.
[
  (required_parameter pattern: [
    (identifier) @name
    (rest_pattern (identifier) @name)
    (object_pattern [
      (shorthand_property_identifier_pattern) @name
      (pair_pattern value: (identifier) @name)
      (rest_pattern (identifier) @name)])
    (array_pattern [
      (identifier) @name
      (rest_pattern (identifier) @name)])])
  (optional_parameter pattern: [
    (identifier) @name
    (object_pattern [
      (shorthand_property_identifier_pattern) @name
      (pair_pattern value: (identifier) @name)
      (rest_pattern (identifier) @name)])
    (array_pattern [
      (identifier) @name
      (rest_pattern (identifier) @name)])])
] @definition.parameter

; A parenthesis-less arrow parameter (`x => x`) is a bare identifier, so the
; definition node and the name node are the same node.
(arrow_function
  parameter: (identifier) @name @definition.parameter)

; --------------------------------------------------------- loop and catch
(for_in_statement
  kind: ["const" "let" "var"]
  left: (identifier) @name @definition.variable)

(for_statement
  initializer: (_
    (variable_declarator
      name: (identifier) @name) @definition.variable))

(catch_clause
  parameter: (identifier) @name @definition.variable)

; ------------------------------------------------------------- members
;
; Methods are captured as functions: the extractor promotes a function inside a
; container to a Method and qualifies it, so `class C { m() {} }` yields `C::m`.
; A getter and a setter of the same name are two declarations and stay two
; symbols — the query language cannot tell them apart from a plain method.
[
  (method_definition
    name: [(property_identifier) (private_property_identifier)] @name)
  (method_signature
    name: [(property_identifier) (private_property_identifier)] @name)
  (abstract_method_signature
    name: [(property_identifier) (private_property_identifier)] @name)
] @definition.function

; A class field holding an arrow (`handleClick = () => {}`) is a method in
; everything but spelling; `!body` keeps the two field rules from overlapping.
(public_field_definition
  name: [(property_identifier) (private_property_identifier)] @name
  value: [(arrow_function) (function_expression) (generator_function)])
@definition.function

[
  (public_field_definition
    name: [(property_identifier) (private_property_identifier)] @name
    value: (_ !body))
  (public_field_definition
    name: [(property_identifier) (private_property_identifier)] @name
    !value)
  (property_signature
    name: [(property_identifier) (private_property_identifier)] @name)
] @definition.field

(enum_assignment
  name: (property_identifier) @name) @definition.field

(enum_body
  name: (property_identifier) @name @definition.field)

; ------------------------------------------------------------- references
(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (member_expression
    property: (property_identifier) @reference.call))

; `new X()` is recorded as a call: it invokes X's constructor, and treating it as
; a call keeps constructors in the call graph.
(new_expression
  constructor: (identifier) @reference.call)

(new_expression
  constructor: (member_expression
    property: (property_identifier) @reference.call))

(member_expression
  property: (property_identifier) @reference.field)

; Every type position — annotations, `implements`, generics, `as` casts — spells
; its target as a type_identifier. `extends` is the exception: a class heritage
; clause holds an expression, so its identifier needs its own rule.
(type_identifier) @reference.type

; `typeof Foo` in a type position names a *value* and reads its type. Without this
; the catch-all still records the use, but as a plain identifier, which understates
; what it is — and an import bound only this way looks value-unused.
(type_query
  (identifier) @reference.type)

(extends_clause
  value: (identifier) @reference.type)

(shorthand_property_identifier) @reference.identifier

; Import and export specifiers name symbols in another module; a rename of the
; target has to rewrite them. The local alias half (`b as c`) names nothing in
; this file's world and is left alone.
(import_specifier
  name: (identifier) @reference.identifier)

(export_specifier
  name: (identifier) @reference.identifier)

; The catch-all is restricted to the `primary_expression` supertype, which means
; identifiers in genuine expression positions. That deliberately excludes JSX
; element names, so a lowercase `<div>` never becomes a symbol reference.
(primary_expression/identifier) @reference.identifier

; ------------------------------------------------------------------- JSX
;
; A capitalised identifier in a `name:` field is a component reference:
; `<MyComponent />` and its closing tag. Lowercase names are HTML tags and are
; not captured at all. The rule is deliberately structural because JSX node
; types cannot be named here, which is also why the captured kind is `identifier`
; and not `type`: the same `name:` field holds capitalised import and export
; specifier names, which are plain value references.
;
; `<ns.Thing />` spells its name as a member expression, so it is captured by the
; member_expression rule above as a field reference instead.
((_ name: (identifier) @reference.identifier)
 (#match? @reference.identifier "^[A-Z]"))

; `className="btn card"` is a cross-language reference to CSS classes — the
; headline case this tool exists for. Written as "an attribute-ish name next to a
; string" so it compiles against both grammars; the same shape also catches
; `{ className: "btn" }` object properties and `className = "btn"` class fields,
; which are equally real references. The whole attribute value is one reference:
; a query cannot split `"btn card"` into two.
((_ (property_identifier) @_attr
    (string (string_fragment) @reference.string))
 (#any-of? @_attr "className" "class" "id"))

; ---------------------------------------------------------------- imports
;
; One Import record per bound name, so `@import.original` always pairs with the
; `@import.name` beside it. A side-effect import binds nothing and is recognised
; by the module string being the statement's first named child.
(import_statement
  .
  (string) @import.path) @import

(import_statement
  (import_clause (identifier) @import.name)
  source: (string) @import.path) @import

(import_statement
  (import_clause
    (namespace_import (identifier) @import.alias @import.name))
  source: (string) @import.path) @import.glob @import

(import_statement
  (import_clause
    (named_imports
      (import_specifier !alias
        name: (identifier) @import.name)))
  source: (string) @import.path) @import

(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @import.original
        alias: (identifier) @import.name)))
  source: (string) @import.path) @import

(import_statement
  (import_require_clause
    (identifier) @import.name
    source: (string) @import.path)) @import

; CommonJS. The module string is what matters; the binding, if any, is already a
; variable definition in its own right.
((call_expression
   function: (identifier) @_require
   arguments: (arguments (string) @import.path)) @import
 (#eq? @_require "require"))

; A re-export is a module dependency exactly like an import, and dropping it
; would lose the edge. The names matter as well: a file of these declares nothing,
; so a name imported from it resolves to no definition there and the declaration is
; one hop further on. Resolution follows that hop.
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @import.name))
  source: (string) @import.path) @import @import.re-export

; `export * from "./x"` names nothing, so only the edge is recorded.
(export_statement
  source: (string) @import.path) @import
