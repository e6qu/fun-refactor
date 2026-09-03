; Lean 4 fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Lean writes a definition and a proof the same way: `def` and `theorem` are two
; spellings of one `definition` node, told apart by its `kind` field. Both are symbols,
; because both are declarations a caller names.
;
; A name resolves through the namespaces `open` brings into scope, which is the shape
; Rust's `use` has, so `open` is an import and `import` names another module.

(module) @scope
(definition) @scope
(structure) @scope
(inductive) @scope
(class_inductive) @scope
(namespace) @scope
(fun) @scope
(let
  body: (_) @scope)
(have) @scope
(match) @scope
(do) @scope
(by) @scope

; `def f (x : Nat) : Nat := …`, and `theorem`, `lemma` and `abbrev` with it.
;
; The anchor takes the last part of the name. `def Box.get` declares `get` in the
; namespace `Box`, and a query binds a field once: without the anchor the declaration
; indexed under `Box` and `get` was nowhere.
(definition
  name: (qualified_name
    [(identifier) (escaped_identifier)] @name .)) @definition.function

; A structure is a record, and its fields qualify under it.
(structure
  name: (identifier) @name @container.name) @definition.struct

(structure
  fields: (structure_field
    name: (identifier) @name) @definition.field)

; An inductive is a sum, and each constructor is one of its variants.
(inductive
  name: (identifier) @name @container.name) @definition.enum

(inductive
  constructors: (constructor
    name: (qualified_name
      [(identifier) (escaped_identifier)] @name .)) @definition.field)

(class_inductive
  name: (identifier) @name @container.name) @definition.enum

(class_inductive
  constructors: (constructor
    name: (qualified_name
      [(identifier) (escaped_identifier)] @name .)) @definition.field)

; `axiom`, `opaque` and `constant` name a value the file does not compute.
(axiom
  name: (qualified_name
    [(identifier) (escaped_identifier)] @name .)) @definition.constant

(opaque
  name: (qualified_name
    [(identifier) (escaped_identifier)] @name .)) @definition.constant

(constant
  name: (qualified_name
    [(identifier) (escaped_identifier)] @name .)) @definition.constant

; A binder names a parameter, whichever bracket holds it.
(explicit_binder
  name: (identifier) @name) @definition.parameter

(implicit_binder
  name: (identifier) @name) @definition.parameter

; A term-local name becomes visible at the expression that follows its value.
(let
  name: (identifier) @name
  body: (_) @binding.body) @definition.variable

; `import Foo` brings the whole of Foo's environment in rather than binding one name,
; which is what a glob import is everywhere else.
(import
  "import" @import.glob
  module: [(identifier) (escaped_identifier) (projection)] @import.path) @import

(open
  namespace: [(identifier) (escaped_identifier) (projection)] @import.path) @import

; `f x` applies `f`. The head of the application is the name a call site spells.
(application
  name: [(identifier) (escaped_identifier)] @reference.call)

; `x.field` and `.ctor` reach a member by name.
(projection
  name: (identifier) @reference.field)

; A type in a binder or a signature names a declaration.
(explicit_binder
  type: (identifier) @reference.type)

(implicit_binder
  type: (identifier) @reference.type)

(structure_field
  type: (identifier) @reference.type)

(definition
  type: (identifier) @reference.type)

(structure
  extends: (identifier) @reference.type)

; Every other identifier reads something this file or an `open` namespace declares.
(identifier) @reference.identifier
