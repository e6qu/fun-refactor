; Rust fact extraction.
; Capture conventions are documented in src/extract.rs.

; ---------------------------------------------------------------- scopes
(block) @scope
(function_item body: (block) @scope)
(closure_expression) @scope
(impl_item) @scope
(trait_item) @scope
(mod_item) @scope
(source_file) @scope

; ------------------------------------------------------------- containers
; An `impl` block qualifies the functions inside it as methods of the type, but
; the type name it mentions is a *reference* to the struct, not a definition of
; it — so this is a container, not a symbol.
(impl_item
  type: (type_identifier) @container.name) @container

(impl_item
  trait: (type_identifier)
  type: (type_identifier) @container.name) @container

(trait_item
  name: (type_identifier) @container.name) @container

; ------------------------------------------------------------ definitions
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

(enum_variant
  name: (identifier) @name) @definition.field

(parameter
  pattern: (identifier) @name) @definition.parameter

(let_declaration
  pattern: (identifier) @name) @definition.variable

; ------------------------------------------------------------- references
(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (scoped_identifier
    name: (identifier) @reference.call))

(call_expression
  function: (field_expression
    field: (field_identifier) @reference.field))

(macro_invocation
  macro: (identifier) @reference.call)

(type_identifier) @reference.type

(field_expression
  field: (field_identifier) @reference.field)

(identifier) @reference.identifier

; ---------------------------------------------------------------- imports
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

(use_declaration
  argument: (use_wildcard) @import.path) @import.glob @import

(extern_crate_declaration
  name: (identifier) @import.path) @import
