; Rust fact extraction.
; Capture conventions are documented in src/extract.rs.

(block) @scope
; The whole item, not only its body: a parameter is declared before the block
; opens, so scoping only the block spilled every parameter into the enclosing
; module. A call to `stmt(...)` then resolved to a sibling function's `stmt`
; parameter whenever that parameter sat nearer than the function it meant.
(function_item) @scope
(function_item body: (block) @scope)
(closure_expression) @scope
(impl_item) @scope
(trait_item) @scope
(mod_item) @scope
(source_file) @scope

; An `impl` block qualifies the functions inside it as methods of the type, but
; the type name it mentions is a *reference* to the struct, not a definition of
; it — so this is a container, not a symbol.
; The enum itself qualifies its variants, exactly as Java's `enum_declaration`
; does its constants.
(enum_item
  name: (type_identifier) @container.name) @container

; A struct qualifies its fields the way Java's class does. Without this a field
; had no owner, so `f.count` on a receiver declared `&Facts` was refused as
; weakly resolved, with a reason naming the very type being renamed.
(struct_item
  name: (type_identifier) @container.name) @container

(impl_item
  type: (type_identifier) @container.name) @container

(impl_item
  trait: (type_identifier)
  type: (type_identifier) @container.name) @container

; `impl Ctx<'_>`, `impl<T> Generic<T>`, `impl Display for Wrapper<T>`. The type node is
; a `generic_type` wrapping the name, so the two patterns above do not match it and the
; methods inside got no container at all — they became plain functions named `run`
; and not `Ctx::run`. `provenance.rs` has one of these and 43 of its methods read as
; dead code, because a `self.hcl_backward(…)` cannot resolve to a symbol that is not a
; member of anything.
(impl_item
  type: (generic_type
    type: (type_identifier) @container.name)) @container

(impl_item
  trait: (_)
  type: (generic_type
    type: (type_identifier) @container.name)) @container

; `impl inner::Deep` and `impl inner::Deep<'_>`. The same gap one node deeper: a path
; puts a `scoped_type_identifier` where the patterns above want a bare name, so the
; methods became functions again. Rust code that spells a type by its path instead of
; importing it is the case.
(impl_item
  type: (scoped_type_identifier
    name: (type_identifier) @container.name)) @container

(impl_item
  trait: (_)
  type: (scoped_type_identifier
    name: (type_identifier) @container.name)) @container

(impl_item
  type: (generic_type
    type: (scoped_type_identifier
      name: (type_identifier) @container.name))) @container

(impl_item
  trait: (_)
  type: (generic_type
    type: (scoped_type_identifier
      name: (type_identifier) @container.name))) @container

(trait_item
  name: (type_identifier) @container.name) @container

; `pub` is captured inside each definition pattern: tree-sitter groups captures
; per pattern match, so a separate visibility pattern would never join the
; definition it belongs to. The `?` quantifier keeps the modifier optional.
(function_item
  (visibility_modifier)? @export
  name: (identifier) @name) @definition.function

(function_signature_item
  name: (identifier) @name) @definition.function

(struct_item
  (visibility_modifier)? @export
  name: (type_identifier) @name) @definition.struct

(enum_item
  (visibility_modifier)? @export
  name: (type_identifier) @name) @definition.enum

(union_item
  (visibility_modifier)? @export
  name: (type_identifier) @name) @definition.struct

(trait_item
  (visibility_modifier)? @export
  name: (type_identifier) @name) @definition.trait

(type_item
  (visibility_modifier)? @export
  name: (type_identifier) @name) @definition.type

(mod_item
  (visibility_modifier)? @export
  name: (identifier) @name) @definition.module

(const_item
  (visibility_modifier)? @export
  name: (identifier) @name) @definition.constant

(static_item
  (visibility_modifier)? @export
  name: (identifier) @name) @definition.constant

(field_declaration
  (visibility_modifier)? @export
  name: (field_identifier) @name) @definition.field

; A variant is reached through its enum, `Shape::Square`, the way Java reaches
; `Suit.HEARTS`. Captured as a field it had no qualifier, so a cross-file
; `Shape::Square(side)` resolved to nothing: every variant of every enum used
; only from other files read as dead code, and this repository's own `Stmt::Let`,
; matched seventeen times in one writer, was listed for deletion.
(enum_variant
  name: (identifier) @name) @definition.constant

(parameter
  pattern: (identifier) @name) @definition.parameter

(let_declaration
  pattern: (identifier) @name) @definition.variable

(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (scoped_identifier
    name: (identifier) @reference.call))

; The callee of a method call is a call, not a field read. `order.name()` and
; `order.name` must record apart: a struct may declare both a field and a method
; under one name, and only the syntax here says which one a use meant.
(call_expression
  function: (field_expression
    field: (field_identifier) @reference.call))

(macro_invocation
  macro: (identifier) @reference.call)

(type_identifier) @reference.type

(field_expression
  field: (field_identifier) @reference.field)

; A destructuring pattern reads the fields it names. `Stmt::ForEach { iterable, .. }`
; is how a writer consumes that field, and it was no reference at all, so every
; field read only by destructuring counted as dead.
(field_pattern
  name: (field_identifier) @reference.field)

(shorthand_field_identifier) @reference.field

; Constructing a value writes its fields: `Facts { kubernetes_objects, .. }` and
; `Stats { imperative_files: n }` are the only places some fields are ever
; written, and neither counted, so a field consumed through serialisation read
; as dead.
(field_initializer
  field: (field_identifier) @reference.field)

; `Facts { kubernetes_objects }`: one identifier, two reads. It reads the local
; and it writes the field, and both must survive or one of the two symbols
; reads as dead. The `.twin` marks the second meaning, so the dedup keeps the
; pair apart without doubling any ordinary span.
(shorthand_field_initializer
  (identifier) @reference.field.twin)


(identifier) @reference.identifier

(use_declaration
  argument: (scoped_identifier) @import.path) @import

(use_declaration
  argument: (identifier) @import.path) @import

(use_declaration
  argument: (use_as_clause
    path: (_) @import.path
    alias: (identifier) @import.alias)) @import

(use_declaration
  argument: (scoped_use_list
    path: (_) @import.path
    list: (use_list (identifier) @import.name))) @import

; `use a::{b as c};` binds `c` to `a::b`. The alias is the local name, and the path it
; renames pairs with it as the original.
(use_declaration
  argument: (scoped_use_list
    path: (_) @import.path
    list: (use_list
      (use_as_clause
        path: (_) @import.original
        alias: (identifier) @import.name)))) @import

; A group nested one level down: `use a::{b::{c, d as e}};`. Rooted at the inner list,
; so the outer brace's depth does not matter. The path is the inner one, which the
; module search resolves the same way it resolves a top-level `use b::c`.
(use_list
  (scoped_use_list
    path: (_) @import.path
    list: (use_list (identifier) @import.name))) @import

(use_list
  (scoped_use_list
    path: (_) @import.path
    list: (use_list
      (use_as_clause
        path: (_) @import.original
        alias: (identifier) @import.name)))) @import

(use_declaration
  argument: (use_wildcard) @import.path) @import.glob @import

(extern_crate_declaration
  name: (identifier) @import.path) @import
