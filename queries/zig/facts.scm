; Zig fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Zig has no dedicated declaration syntax for types: `struct`, `union`, `enum`,
; `opaque` and `error` sets are *expressions* bound by a `const`. The name a rename
; rewrites is therefore the const's identifier, and the whole `const … ;` statement
; is the definition.
;
; `pub`, `export` and `extern` are anonymous tokens, so visibility is captured with
; a quantified anonymous pattern inside each definition rather than a separate rule:
; captures only group within a single match.

; ---------------------------------------------------------------- scopes
(source_file) @scope
(block) @scope
(function_declaration) @scope
(test_declaration) @scope
(struct_declaration) @scope
(union_declaration) @scope
(enum_declaration) @scope
(opaque_declaration) @scope
(if_expression) @scope
(if_statement) @scope
(for_expression) @scope
(for_statement) @scope
(while_expression) @scope
(while_statement) @scope
(switch_expression) @scope
(switch_case) @scope

; ------------------------------------------------------------- containers
; `const Point = struct { … }` binds a container to a name. Functions declared
; inside it are methods of `Point`, and its fields qualify as `Point::x`. The
; identifier is the type's one definition site, so the container reuses it rather
; than declaring a second symbol.
(variable_declaration
  "const"
  .
  (identifier) @container.name
  "="
  [
    (struct_declaration)
    (union_declaration)
    (enum_declaration)
    (opaque_declaration)
    (error_set_declaration)
  ]) @container

; ------------------------------------------------------------ definitions
(function_declaration
  "pub"? @export
  "export"? @export
  name: (identifier) @name) @definition.function

; A prototype with no body — `extern fn` and function-typed fields.
(function_signature
  name: (identifier) @name) @definition.function

; `test "name" { … }`. The quotes are not part of the name a rename would rewrite.
(test_declaration
  "pub"? @export
  (string
    (string_content) @name)) @definition.function

; `test someDecl { … }` names a declaration to exercise rather than introducing a
; name of its own, so the identifier stays a reference and the block is not a
; symbol. The catch-all reference rule below picks it up.

(variable_declaration
  "pub"? @export
  "export"? @export
  "const"
  .
  (identifier) @name
  "="
  (struct_declaration)) @definition.struct

; The model has no separate union kind; a Zig union is a tagged record like a struct.
(variable_declaration
  "pub"? @export
  "export"? @export
  "const"
  .
  (identifier) @name
  "="
  (union_declaration)) @definition.struct

(variable_declaration
  "pub"? @export
  "export"? @export
  "const"
  .
  (identifier) @name
  "="
  (opaque_declaration)) @definition.struct

(variable_declaration
  "pub"? @export
  "export"? @export
  "const"
  .
  (identifier) @name
  "="
  (enum_declaration)) @definition.enum

; An error set is a closed set of named values — an enum in everything but spelling.
(variable_declaration
  "pub"? @export
  "export"? @export
  "const"
  .
  (identifier) @name
  "="
  (error_set_declaration)) @definition.enum

; Plain `const`. The query language cannot negate a child pattern, so the value's
; text is what keeps this from firing a second time on the container declarations
; above. The `error` alternative is closed with `$` so that an error-set *merge*
; (`error{A} || error{B}`, a binary expression) still counts as an ordinary const
; rather than being dropped by both rules.
((variable_declaration
   "pub"? @export
   "export"? @export
   "const"
   .
   (identifier) @name
   "="
   (_) @_value) @definition.constant
 (#not-match? @_value "^((extern|packed)\\s+)?(struct|union|enum|opaque)\\b|^error\\s*\\{[^|]*\\}$"))

(variable_declaration
  "pub"? @export
  "export"? @export
  "var"
  .
  (identifier) @name
  "=") @definition.variable

; `extern` declarations name a symbol defined elsewhere and carry no initialiser,
; so the value-shaped patterns above cannot see them. An explicit type is mandatory
; on an `extern`, and requiring it here is what keeps the annotation itself from
; being read as the declared name — an anchor could not, because `extern "c"` puts
; a string ahead of the identifier.
(variable_declaration
  "pub"? @export
  "extern" @export
  "const"
  (identifier) @name
  type: (_)) @definition.constant

(variable_declaration
  "pub"? @export
  "extern" @export
  "var"
  (identifier) @name
  type: (_)) @definition.variable

(container_field
  name: (identifier) @name) @definition.field

; Each member of `error{ A, B }` is a value of the set. The member *is* the whole
; definition, so name and full span coincide.
(error_set_declaration
  (identifier) @name @definition.field)

(parameter
  name: (identifier) @name) @definition.parameter

; `if (x) |value|`, `for (xs) |item, i|`, `catch |err|` — payloads bind names.
(payload
  (identifier) @name) @definition.variable

; ------------------------------------------------------------- references
(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (field_expression
    member: (identifier) @reference.call))

(field_expression
  member: (identifier) @reference.field)

; Zig writes types as ordinary expressions, so a type position is the only signal.
(parameter
  type: (identifier) @reference.type)

(function_declaration
  type: (identifier) @reference.type)

(function_signature
  type: (identifier) @reference.type)

(variable_declaration
  type: (identifier) @reference.type)

(container_field
  type: (identifier) @reference.type)

(struct_initializer
  (identifier) @reference.type)

; A type written as `*T`, `[]T`, `?T` or `E!T` still names T in type position.
(pointer_type
  (identifier) @reference.type)

(slice_type
  (identifier) @reference.type)

(nullable_type
  (identifier) @reference.type)

(error_union_type
  ok: (identifier) @reference.type)

(error_union_type
  error: (identifier) @reference.type)

(identifier) @reference.identifier

; ---------------------------------------------------------------- imports
; `const std = @import("std");` — the const's identifier is the locally bound name
; and the builtin's string argument is the path.
((variable_declaration
   "pub"? @export
   "const"
   .
   (identifier) @import.name
   "="
   (builtin_function
     (builtin_identifier) @_builtin
     (arguments
       (string) @import.path))) @import
 (#eq? @_builtin "@import"))

; `const mem = @import("std").mem;` binds one member of the imported namespace.
((variable_declaration
   "pub"? @export
   "const"
   .
   (identifier) @import.name
   "="
   (field_expression
     object: (builtin_function
       (builtin_identifier) @_builtin
       (arguments
         (string) @import.path))
     member: (identifier) @import.original)) @import
 (#eq? @_builtin "@import"))
