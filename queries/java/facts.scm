; Java fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Java says "externally visible" with a keyword and not with capitalisation, and
; `modifiers` is an optional *positional* child and not a field — so `!modifiers`
; is not available and the three cases have to be made mutually exclusive by hand. A
; match fires whole or not at all, and nothing downstream de-duplicates definitions, so
; two patterns that can both match one declaration produce two symbols sharing a name
; span, which is a rename that rewrites the same bytes twice.
;
; The three cases per declaration are:
;
;   1. has modifiers, one of which is `public`   -> exported
;   2. has modifiers, none of which is `public`  -> not exported
;   3. has no modifiers at all (package-private) -> not exported
;
; The third is written with the `.` anchor before the keyword, which means "this is the
; first child" and therefore cannot hold for a declaration that has modifiers.
;
; `protected` is deliberately not `@export`: it is visible to subclasses and to the
; package, not to a caller who imports the type, and this flag decides whether a rename
; has to look outside the file.

; ---------------------------------------------------------------- scopes
(program) @scope
(class_body) @scope
(interface_body) @scope
(enum_body) @scope
(block) @scope
(constructor_body) @scope
(lambda_expression) @scope
(for_statement) @scope
(enhanced_for_statement) @scope
(while_statement) @scope
(do_statement) @scope
(if_statement) @scope
(switch_block) @scope
(try_statement) @scope
(catch_clause) @scope
(finally_clause) @scope

; ------------------------------------------------------------- containers
; A type qualifies the methods and fields declared inside it, so `Account::getOwner`
; is distinguishable from any other `getOwner` in the workspace.
(class_declaration name: (identifier) @container.name) @container
(interface_declaration name: (identifier) @container.name) @container
(enum_declaration name: (identifier) @container.name) @container
(record_declaration name: (identifier) @container.name) @container

; ------------------------------------------------------------ definitions
; The package clause names the compilation unit every importer refers to.
(package_declaration
  (_) @name) @definition.module

; ---- classes
((class_declaration
   (modifiers) @export
   name: (identifier) @name) @definition.class
 (#match? @export "public"))

((class_declaration
   (modifiers) @mods
   name: (identifier) @name) @definition.class
 (#not-match? @mods "public"))

(class_declaration
  . "class"
  name: (identifier) @name) @definition.class

; ---- interfaces
((interface_declaration
   (modifiers) @export
   name: (identifier) @name) @definition.interface
 (#match? @export "public"))

((interface_declaration
   (modifiers) @mods
   name: (identifier) @name) @definition.interface
 (#not-match? @mods "public"))

(interface_declaration
  . "interface"
  name: (identifier) @name) @definition.interface

; ---- enums
((enum_declaration
   (modifiers) @export
   name: (identifier) @name) @definition.enum
 (#match? @export "public"))

((enum_declaration
   (modifiers) @mods
   name: (identifier) @name) @definition.enum
 (#not-match? @mods "public"))

(enum_declaration
  . "enum"
  name: (identifier) @name) @definition.enum

(enum_constant
  name: (identifier) @name) @definition.constant

; ---- records: a class whose fields are its parameters
((record_declaration
   (modifiers) @export
   name: (identifier) @name) @definition.class
 (#match? @export "public"))

((record_declaration
   (modifiers) @mods
   name: (identifier) @name) @definition.class
 (#not-match? @mods "public"))

(record_declaration
  . "record"
  name: (identifier) @name) @definition.class

; ---- methods
((method_declaration
   (modifiers) @export
   name: (identifier) @name) @definition.method
 (#match? @export "public"))

((method_declaration
   (modifiers) @mods
   name: (identifier) @name) @definition.method
 (#not-match? @mods "public"))

; A method with no modifiers at all — an interface method, or a package-private one.
; The first child is its return type, and each shape is listed and not matched
; with `(_)`, which would also match the `modifiers` node and undo the exclusion.
(method_declaration
  . [
    (void_type)
    (type_identifier)
    (integral_type)
    (floating_point_type)
    (boolean_type)
    (generic_type)
    (array_type)
    (scoped_type_identifier)
  ]
  name: (identifier) @name) @definition.method

; ---- constructors
((constructor_declaration
   (modifiers) @export
   name: (identifier) @name) @definition.method
 (#match? @export "public"))

((constructor_declaration
   (modifiers) @mods
   name: (identifier) @name) @definition.method
 (#not-match? @mods "public"))

(constructor_declaration
  . name: (identifier) @name) @definition.method

; ---- fields
((field_declaration
   (modifiers) @export
   declarator: (variable_declarator
     name: (identifier) @name)) @definition.field
 (#match? @export "public"))

((field_declaration
   (modifiers) @mods
   declarator: (variable_declarator
     name: (identifier) @name)) @definition.field
 (#not-match? @mods "public"))

(field_declaration
  . type: (_)
  declarator: (variable_declarator
    name: (identifier) @name)) @definition.field

; ---- locals and parameters, never visible outside the method that declares them
; The declarator and not the statement. One statement may declare several names —
; `int a = 1, b = 2, c = 3;` — and capturing the statement gave all three the same span,
; so each of them claimed the other two: inlining `b` took `a`'s value and deleted the
; whole line.
(local_variable_declaration
  declarator: (variable_declarator
    name: (identifier) @name) @definition.variable)

(formal_parameter
  name: (identifier) @name) @definition.parameter

(spread_parameter
  (variable_declarator
    name: (identifier) @name)) @definition.parameter

(catch_formal_parameter
  name: (identifier) @name) @definition.parameter

(enhanced_for_statement
  name: (identifier) @name) @definition.variable

(type_parameter
  (type_identifier) @name) @definition.parameter

; ------------------------------------------------------------- references
(method_invocation
  name: (identifier) @reference.call)

(object_creation_expression
  type: (type_identifier) @reference.type)

(field_access
  field: (identifier) @reference.field)

(type_identifier) @reference.type

(identifier) @reference.identifier

; ---------------------------------------------------------------- imports
; `import a.b.C;` binds the last segment, which is the name the file then writes.
(import_declaration
  (scoped_identifier
    name: (identifier) @import.name) @import.path) @import

; `import a.b.*;` brings every public type in the package into scope.
(import_declaration
  (scoped_identifier) @import.path
  (asterisk) @import.glob) @import
