; Sass fact extraction, for the indented syntax of `.sass` files.
; Capture conventions are documented in src/extract.rs.
;
; Sass has two syntaxes and this is the older one: blocks are indentation and statements
; end at the line. The grammar is therefore a different grammar, with node names of its
; own, and this file is where they are read. Everything it declares is what
; queries/scss/facts.scm declares, under the same kinds and the same names, because a
; `.sass` file and a `.scss` file in one workspace name each other's selectors,
; variables and mixins.
;
; Naming notes (asserted in tests/facts_sass.rs):
;   * `.btn` -> name is `btn`; the leading `.` is outside the name span.
;   * `#main` -> name is `main`; the leading `#` is outside the name span.
;   * `--brand-color` -> name is `--brand-color`, dashes included.
;   * `$brand` -> name is `$brand`, dollar included: that is the spelling at both the
;     declaration and every use site, so a rename rewrites matching text on both sides.
;
; Every selector occurrence is a definition site, as it is in CSS: `.btn` written twice
; declares the same class twice, and renaming it must rewrite all of them.

; A block is what nests declarations inside rules and keyframes. A mixin or function body
; additionally has its parameters in scope, and those sit outside the `block` node, so the
; statement itself opens the scope that holds them.
(stylesheet) @scope
(block) @scope
(mixin_statement) @scope
(function_statement) @scope

; Selectors.
(class_selector
  (class_name) @name) @definition.selector

(id_selector
  (id_name) @name) @definition.element-id

(placeholder_selector
  (placeholder_name) @name) @definition.selector

; `@keyframes slide` names a CSS identifier that `animation-name` refers to. It is filed
; under `selector` because that is this model's kind for a name in the CSS identifier
; namespace.
(keyframes_statement
  name: (keyframes_name) @name) @definition.selector

; A custom property, `--brand-color: red`.
(declaration
  (property_name) @name
  (#match? @name "^--")) @definition.property

; A Sass variable, `$brand: #fff`. The grammar gives the declaration's name its own node,
; so a use site cannot be mistaken for one.
(declaration
  (variable_name) @name) @definition.property

; `@mixin theme($c)` and `@function double($n)`. Both are callable and both carry a
; parameter list, which is what makes a signature change possible here at all.
(mixin_statement
  (name) @name) @definition.function

(function_statement
  (name) @name) @definition.function

; A parameter of either.
(parameters
  (parameter
    (variable_name) @name)) @definition.parameter

; The name a `@use` binds its module to, `@use "theme" as t`.
(use_statement
  (as_clause
    (use_alias) @name)) @definition.module

; `$brand` read anywhere: a declaration value, an argument, an interpolation, and through
; the namespace a `@use` gave it.
(variable_value) @reference.identifier

; `var(--brand-color)`: the argument names a custom property.
(var_function
  (custom_property_name) @reference.identifier)

; `double(3)` calls a Sass function, and `t.double(3)` calls it through a namespace. The
; name alone is the reference; the namespace is a module of its own.
(call_expression
  name: (function_name) @reference.call
  (#not-eq? @reference.call "var"))

(module) @reference.identifier

; `@include theme(red)` calls a mixin. This is the call site a signature change rewrites,
; so it is a call and not a plain identifier use.
(include_statement
  (mixin_name) @reference.call)

; `@extend .other` and `@extend %placeholder` need no pattern of their own: the selector
; patterns above match the occurrence, and in CSS every occurrence of a selector is a
; definition of it. A rename rewrites all of them, which is what `@extend` needs.

; `animation-name: slide` and `animation: slide 1s` refer to a `@keyframes` name.
(declaration
  (property_name) @_prop
  (plain_value) @reference.identifier
  (#any-of? @_prop "animation-name" "animation"))

; `@import "other"`, `@use "buttons"` and `@forward "buttons"`. The engine strips the
; quotes from an import path.
;
; What each one makes visible differs, and resolution reads it from these captures. A
; `@use` binds one namespace: `theme.$brand`, or `t.$brand` after `as t`. `@use ... as *`
; and the older `@import` bind every name the other file declares, with no namespace at
; all, which is what a glob import is.
(import_statement
  (string_value) @import.path) @import.glob @import

; `as t` binds one namespace and `as *` binds none. This grammar spells both as a
; `use_alias`, so the star is told from a name by what it says.
(use_statement
  (string_value) @import.path
  (as_clause
    (use_alias) @import.alias)?
  (#not-eq? @import.alias "*")) @import

(use_statement
  (string_value) @import.path
  (as_clause
    (use_alias) @import.glob)
  (#eq? @import.glob "*")) @import

; `@forward "buttons"` re-exports what that file declares: a third file that `@use`s
; this one reaches them through this file's namespace.
(forward_statement
  (string_value) @import.path) @import.re-export @import
