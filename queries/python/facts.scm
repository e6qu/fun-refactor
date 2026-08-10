; Python fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Visibility. Python has no visibility keywords; the convention is that a leading
; underscore means "internal". A definition is therefore captured as @export
; exactly when its name does not start with `_`, which takes two mutually
; exclusive patterns per kind: a predicate can filter a match but cannot switch a
; capture on and off inside one pattern. The `#match?` / `#not-match?` pair keeps
; the two from both firing, which would define the same symbol twice. Dunder
; names (`__init__`) start with `_` and are consequently not exported.
;
; Decorators. `@deco` + `def f(): ...` parses as
; (decorated_definition (decorator) definition: (function_definition)), and what
; is captured below is the inner function_definition: full_span starts at `def`
; (or `class`) and excludes the decorator lines. Capturing decorated_definition
; as well would need a second pattern matching the same function, which would
; define every decorated function twice. name_span — the bytes a rename rewrites
; — is identical either way; only a whole-definition move would want the
; decorators, and it can walk up one node to find them.

; ---------------------------------------------------------------- scopes
; Only the module, functions, lambdas and comprehensions introduce a scope in
; Python: an `if` or `for` block does not, so their blocks are deliberately not
; captured. A class body is a scope of its own that nested functions do not see
; through, which is exactly the containment relation @scope encodes.
(module) @scope
(function_definition body: (block) @scope)
(class_definition body: (block) @scope)
(lambda) @scope
(list_comprehension) @scope
(set_comprehension) @scope
(dictionary_comprehension) @scope
(generator_expression) @scope

; ------------------------------------------------------------- containers
; A class qualifies the functions inside it as methods. Unlike a Rust `impl`,
; the class is also a definition in its own right, so the same node is captured
; twice: once as @container here and once as @definition.class below. The
; qualifier lookup ignores a container whose name span is the definition's own,
; so the class does not end up qualifying itself.
(class_definition
  name: (identifier) @container.name) @container

; ------------------------------------------------------------ definitions
((function_definition
   name: (identifier) @name @export) @definition.function
 (#not-match? @name "^_"))

((function_definition
   name: (identifier) @name) @definition.function
 (#match? @name "^_"))

((class_definition
   name: (identifier) @name @export) @definition.class
 (#not-match? @name "^_"))

((class_definition
   name: (identifier) @name) @definition.class
 (#match? @name "^_"))

; PEP 695 type alias: `type Alias = int`.
((type_alias_statement
   left: (type (identifier) @name @export)) @definition.type
 (#not-match? @name "^_"))

((type_alias_statement
   left: (type (identifier) @name)) @definition.type
 (#match? @name "^_"))

; ------------------------------------------------------------- parameters
; One pattern per parameter form: plain, annotated, defaulted, annotated and
; defaulted, `*args` and `**kwargs` (each of the last two also in an annotated
; form, where the splat pattern sits inside a typed_parameter). The forms nest
; and not overlap, so no parameter is defined twice.
(parameters (identifier) @name @definition.parameter)
(lambda_parameters (identifier) @name @definition.parameter)
(typed_parameter (identifier) @name) @definition.parameter
(default_parameter name: (identifier) @name) @definition.parameter
(typed_default_parameter name: (identifier) @name) @definition.parameter
(parameters (list_splat_pattern (identifier) @name) @definition.parameter)
(parameters (dictionary_splat_pattern (identifier) @name) @definition.parameter)
(typed_parameter (list_splat_pattern (identifier) @name)) @definition.parameter
(typed_parameter (dictionary_splat_pattern (identifier) @name)) @definition.parameter

; --------------------------------------------------- module-level bindings
; A module-level assignment binds a module attribute. ALL_CAPS is the constant
; convention; anything else is a plain variable. Crossed with the underscore
; visibility convention, that makes four mutually exclusive patterns.
((module (expression_statement
   (assignment left: (identifier) @name @export) @definition.constant))
 (#match? @name "^[A-Z_][A-Z0-9_]*$")
 (#not-match? @name "^_"))

((module (expression_statement
   (assignment left: (identifier) @name) @definition.constant))
 (#match? @name "^[A-Z_][A-Z0-9_]*$")
 (#match? @name "^_"))

((module (expression_statement
   (assignment left: (identifier) @name @export) @definition.variable))
 (#not-match? @name "^[A-Z_][A-Z0-9_]*$")
 (#not-match? @name "^_"))

((module (expression_statement
   (assignment left: (identifier) @name) @definition.variable))
 (#not-match? @name "^[A-Z_][A-Z0-9_]*$")
 (#match? @name "^_"))

; ----------------------------------------------------------- class fields
; An assignment directly in a class body is a class attribute, annotated
; (`x: int = 1`) or not.
((class_definition body: (block (expression_statement
   (assignment left: (identifier) @name @export) @definition.field)))
 (#not-match? @name "^_"))

((class_definition body: (block (expression_statement
   (assignment left: (identifier) @name) @definition.field)))
 (#match? @name "^_"))

; ---------------------------------------------------------------- locals
; A local is an assignment inside a block. The pattern is repeated once per
; construct that owns a block instead of being written against (block) directly,
; because the one block that must NOT be matched here is a class body: its
; assignments are fields, and a generic (block ...) pattern would define them a
; second time as variables. Locals are never exported.
(function_definition
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(if_statement
  consequence: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(elif_clause
  consequence: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(else_clause
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(for_statement
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(while_statement
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(with_statement
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(try_statement
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(except_clause
  (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(finally_clause
  (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(match_statement
  body: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))
(case_clause
  consequence: (block (expression_statement (assignment left: (identifier) @name) @definition.variable)))

; ------------------------------------------------- other binding forms
; A loop variable, a `with ... as f` / `except ... as e` binding and a walrus
; are all bindings whose name is the whole definition: capturing the identifier
; as both @name and @definition keeps full_span off the loop body, which would
; otherwise appear to contain — and so own — every symbol nested in it.
(for_statement left: (identifier) @name @definition.variable)
(for_statement left: (pattern_list (identifier) @name @definition.variable))
(for_in_clause left: (identifier) @name @definition.variable)
(for_in_clause left: (pattern_list (identifier) @name @definition.variable))
; The `as` target is an aliased expression node, so `with cm as obj.attr` and
; `with cm as (a, b)` reach this pattern too. Only a bare identifier names a
; symbol; the predicate keeps the other forms from defining one called
; "obj.attr". Their parts are still visible as references.
((as_pattern alias: (as_pattern_target) @name @definition.variable)
 (#match? @name "^[A-Za-z_][A-Za-z0-9_]*$"))
(named_expression name: (identifier) @name @definition.variable)

; ------------------------------------------------------------- references
(call
  function: (identifier) @reference.call)

(call
  function: (attribute
    attribute: (identifier) @reference.call))

; A decorator names a callable, whether or not it is called with arguments.
(decorator
  (identifier) @reference.call)

(decorator
  (attribute
    attribute: (identifier) @reference.call))

(attribute
  attribute: (identifier) @reference.field)

(type (identifier) @reference.type)
(generic_type (identifier) @reference.type)

(class_definition
  superclasses: (argument_list (identifier) @reference.type))

; `global x` / `nonlocal x` do not define anything: they rebind a name defined in
; an enclosing (or module) scope, so the names are use sites. A rename of the
; target variable has to rewrite them, which it does through these. The
; assignment that follows such a declaration is still captured as a local by the
; patterns above: correlating it with the declaration is a scope-analysis job,
; not something a pattern can decide.
(global_statement (identifier) @reference.identifier)
(nonlocal_statement (identifier) @reference.identifier)

(identifier) @reference.identifier

; ---------------------------------------------------------------- imports
; `import os` / `import os.path`
(import_statement
  name: (dotted_name) @import.path) @import

; `import os.path as p`
(import_statement
  name: (aliased_import
    name: (dotted_name) @import.path
    alias: (identifier) @import.alias)) @import

; `from m import a` — also `from . import a` and `from .rel import a`, whose
; module_name is a relative_import whose text is the leading dots plus module.
; One Import is produced per imported name, so a statement importing several
; names yields several Imports sharing a path.
(import_from_statement
  module_name: (_) @import.path
  name: (dotted_name (identifier) @import.name)) @import

; `from m import a as b`: @import.original pairs positionally with @import.name.
(import_from_statement
  module_name: (_) @import.path
  name: (aliased_import
    name: (dotted_name (identifier) @import.original)
    alias: (identifier) @import.name)) @import

; `from m import *`
(import_from_statement
  module_name: (_) @import.path
  (wildcard_import) @import.glob) @import

; `from __future__ import annotations`
(future_import_statement
  "__future__" @import.path
  name: (dotted_name (identifier) @import.name)) @import
