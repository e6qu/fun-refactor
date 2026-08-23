; Go fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Go has no visibility keyword: an identifier is exported exactly when it begins
; with an upper-case letter. `@export` only counts when captured inside the
; definition's own match, and a match either fires whole or not at all, so every
; exported definition needs a `#match?` pattern plus its `#not-match?` twin.
;
; Definitions sit on the *spec* node (`type_spec`, `const_spec`, `var_spec`) rather
; than the enclosing declaration, because a grouped `const ( A = 1; B = 2 )` holds
; several definitions. The `type` / `const` / `var` keyword therefore falls outside
; `full_span` for single-spec declarations.

(source_file) @scope
(block) @scope
(func_literal) @scope
(function_declaration) @scope
(method_declaration) @scope
(if_statement) @scope
(for_statement) @scope
(expression_switch_statement) @scope
(type_switch_statement) @scope
(select_statement) @scope
(expression_case) @scope
(type_case) @scope
(communication_case) @scope
(default_case) @scope

; The receiver type qualifies a method as `T::m`. The `T` written in the receiver
; is a reference to the type declared elsewhere, never a second definition of it,
; which is exactly what `@container` expresses. Receivers come in four shapes:
; `T`, `*T`, `T[P]` and `*T[P]`.
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: (type_identifier) @container.name))) @container

(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: (pointer_type
        (type_identifier) @container.name)))) @container

(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: (generic_type
        type: (type_identifier) @container.name)))) @container

(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: (pointer_type
        (generic_type
          type: (type_identifier) @container.name))))) @container

; A named struct or interface qualifies the fields and methods declared inside it.
(type_spec
  name: (type_identifier) @container.name
  type: (struct_type)) @container

(type_spec
  name: (type_identifier) @container.name
  type: (interface_type)) @container

; The package clause names the compilation unit every importer refers to, so it is
; exported by construction and not by capitalisation.
(package_clause
  "package" @export
  (package_identifier) @name) @definition.module

((function_declaration
   name: (identifier) @name @export) @definition.function
 (#match? @export "^[A-Z]"))

((function_declaration
   name: (identifier) @name) @definition.function
 (#not-match? @name "^[A-Z]"))

((method_declaration
   name: (field_identifier) @name @export) @definition.method
 (#match? @export "^[A-Z]"))

((method_declaration
   name: (field_identifier) @name) @definition.method
 (#not-match? @name "^[A-Z]"))

; Interface methods are declarations too, qualified by the interface container.
((method_elem
   name: (field_identifier) @name @export) @definition.method
 (#match? @export "^[A-Z]"))

((method_elem
   name: (field_identifier) @name) @definition.method
 (#not-match? @name "^[A-Z]"))

((type_spec
   name: (type_identifier) @name @export
   type: (struct_type)) @definition.struct
 (#match? @export "^[A-Z]"))

((type_spec
   name: (type_identifier) @name
   type: (struct_type)) @definition.struct
 (#not-match? @name "^[A-Z]"))

((type_spec
   name: (type_identifier) @name @export
   type: (interface_type)) @definition.interface
 (#match? @export "^[A-Z]"))

((type_spec
   name: (type_identifier) @name
   type: (interface_type)) @definition.interface
 (#not-match? @name "^[A-Z]"))

; Every remaining `_type` the grammar admits. Listing them explicitly is what stops
; this pattern from firing a second time on the struct and interface specs above:
; the query language cannot negate a child pattern.
((type_spec
   name: (type_identifier) @name @export
   type: [
     (array_type)
     (channel_type)
     (function_type)
     (generic_type)
     (map_type)
     (negated_type)
     (parenthesized_type)
     (pointer_type)
     (qualified_type)
     (slice_type)
     (type_identifier)
   ]) @definition.type
 (#match? @export "^[A-Z]"))

((type_spec
   name: (type_identifier) @name
   type: [
     (array_type)
     (channel_type)
     (function_type)
     (generic_type)
     (map_type)
     (negated_type)
     (parenthesized_type)
     (pointer_type)
     (qualified_type)
     (slice_type)
     (type_identifier)
   ]) @definition.type
 (#not-match? @name "^[A-Z]"))

((type_alias
   name: (type_identifier) @name @export) @definition.type
 (#match? @export "^[A-Z]"))

((type_alias
   name: (type_identifier) @name) @definition.type
 (#not-match? @name "^[A-Z]"))

((const_spec
   name: (identifier) @name @export) @definition.constant
 (#match? @export "^[A-Z]"))

((const_spec
   name: (identifier) @name) @definition.constant
 (#not-match? @name "^[A-Z]"))

((var_spec
   name: (identifier) @name @export) @definition.variable
 (#match? @export "^[A-Z]"))

((var_spec
   name: (identifier) @name) @definition.variable
 (#not-match? @name "^[A-Z]"))

((field_declaration
   name: (field_identifier) @name @export) @definition.field
 (#match? @export "^[A-Z]"))

((field_declaration
   name: (field_identifier) @name) @definition.field
 (#not-match? @name "^[A-Z]"))

; Locals and parameters are never visible outside their function, so they carry no
; export test regardless of their capitalisation.
(parameter_declaration
  name: (identifier) @name) @definition.parameter

(variadic_parameter_declaration
  name: (identifier) @name) @definition.parameter

(type_parameter_declaration
  name: (identifier) @name) @definition.parameter

(short_var_declaration
  left: (expression_list
    (identifier) @name)) @definition.variable

(range_clause
  left: (expression_list
    (identifier) @name)) @definition.variable

(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (parenthesized_expression
    (identifier) @reference.call))

; `pkg.Fn()` and `x.M()` are the same syntax; the selector's field is the callee.
(call_expression
  function: (selector_expression
    field: (field_identifier) @reference.call))

(call_expression
  function: (parenthesized_expression
    (selector_expression
      field: (field_identifier) @reference.call)))

(selector_expression
  field: (field_identifier) @reference.field)

; `Point{X: 1}` names a field of the composite literal's type.
(keyed_element
  key: (literal_element
    (identifier) @reference.field))

(type_identifier) @reference.type

(field_identifier) @reference.field

(package_identifier) @reference.identifier

(identifier) @reference.identifier

; One `Import` per spec, so a grouped `import ( ... )` reports each path with its
; own span instead of one span covering the whole block.
(import_spec
  !name
  path: (_) @import.path) @import

(import_spec
  name: (package_identifier) @import.alias
  path: (_) @import.path) @import

; `import _ "embed"` binds nothing; the alias records the blank so callers can tell
; a side-effect-only import from a named one.
(import_spec
  name: (blank_identifier) @import.alias
  path: (_) @import.path) @import

; `import . "math"` drops every exported name into file scope — a glob import.
(import_spec
  name: (dot) @import.glob
  path: (_) @import.path) @import
