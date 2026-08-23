; SCSS fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; SCSS has its own grammar (tree-sitter-scss), so everything CSS declares is here
; unchanged plus the constructs that are Sass-only: `$variables`, `@mixin`/`@include`,
; `@function`, and the `@use`/`@forward` module system. The CSS half is kept in step
; with queries/css/facts.scm deliberately — an SCSS file is a CSS file first.
;
; Naming notes (asserted in tests/facts_css.rs):
;   * `.btn` -> name is `btn`; the leading `.` is outside the name span.
;   * `#main` -> name is `main`; the leading `#` is outside the name span.
;   * `--brand-color` -> name is `--brand-color`, dashes included.
;   * `$brand` -> name is `$brand`, dollar included: that is the spelling at both the
;     declaration and every use site, so a rename rewrites matching text on both sides.
;     A mixin parameter is spelled the same way and is a definition of the same shape.
;
; Every selector occurrence is a definition site. CSS has no single canonical
; definition of a class: `.btn { }` written twice declares the same class twice,
; and renaming it must rewrite all of them.

; Block structure is what nests declarations inside rules, media queries and
; keyframes. A mixin or function body additionally has its parameters in scope, and
; those sit outside the `block` node, so the statement itself opens the scope that
; holds them — without it, two mixins that both take `$c` would be ambiguous.
(stylesheet) @scope
(block) @scope
(keyframe_block_list) @scope
(mixin_statement) @scope
(function_statement) @scope

; Class selectors. `class_name` is also used by the grammar for pseudo-classes
; (`:hover`, `:root`), so the pattern is anchored on `class_selector` to keep
; pseudo-classes out of the symbol table.
(class_selector
  (class_name) @name) @definition.selector

; Id selectors. Kind is element-id, matching HTML/XML `id="..."`: the two are the
; same entity seen from opposite sides, and one shared kind lets the index pair
; them up by (kind, name).
(id_selector
  (id_name) @name) @definition.element-id

; `@keyframes slide` names a CSS identifier referenced by `animation-name`. It is
; not a selector in the CSS sense; it is filed under `selector` because that is
; this model's kind for "a name in the CSS identifier namespace".
(keyframes_statement
  (keyframes_name) @name) @definition.selector

; Custom property declaration: `--brand-color: red`.
(declaration
  (property_name) @name
  (#match? @name "^--")) @definition.property

; SCSS variable declaration: `$brand: #fff`. The grammar spells it as an ordinary
; declaration whose property name starts with a dollar, which is also why it shares
; a kind with custom properties: both are a name bound to a value and substituted
; into later declarations.
(declaration
  (property_name) @name
  (#match? @name "^\\$")) @definition.property

; `@namespace svg url(...)` binds a namespace prefix usable in selectors.
(namespace_statement
  (namespace_name) @name) @definition.module

; `@mixin theme($c) { ... }` and `@function double($n) { ... }`. Both are callable
; and both carry a parameter list, which is what makes a signature change possible
; here at all.
(mixin_statement
  name: (identifier) @name) @definition.function

(function_statement
  name: (identifier) @name) @definition.function

; A parameter of either.
(parameters
  (parameter
    (variable) @name
    (#match? @name "^\\$"))) @definition.parameter

; `var(--brand-color)` and `var(--brand-color, fallback)`: only arguments spelled
; like a custom property are references; the fallback value is not.
(call_expression
  (function_name) @_fn
  (arguments
    (plain_value) @reference.identifier)
  (#eq? @_fn "var")
  (#match? @reference.identifier "^--"))

; `$brand` read anywhere: a declaration value, an argument, an interpolation. A
; capture that lands on a declaration's or parameter's own name is dropped by the
; extractor, so this pattern does not have to exclude definition sites itself.
(variable) @reference.identifier
  (#match? @reference.identifier "^\\$")

; `@include theme(red)` calls a mixin. This is the call site a signature change
; rewrites, so it is a call and not a plain identifier use. A namespaced include
; names the same mixin through the namespace a `@use` gave it, so the name alone
; is the reference and the namespace is not.
(include_statement
  (identifier) @reference.call)

(include_statement
  (namespaced_name
    name: (identifier) @reference.call))

; `double(3)` calls a Sass function. `var()` is excluded because it is CSS's
; custom-property lookup instead of a function anyone declares, and it already has
; its own pattern above; every other name here is either a user `@function` or a
; built-in that simply resolves to nothing.
(call_expression
  (function_name) @reference.call
  (#not-eq? @reference.call "var"))

; `animation-name: slide` / `animation: slide 1s` refer to a @keyframes name.
(declaration
  (property_name) @_prop
  (plain_value) @reference.identifier
  (#any-of? @_prop "animation-name" "animation"))

; `svg|circle` uses a prefix bound by `@namespace`. The grammar spells both sides
; as `tag_name`, so the anchor picks the prefix and leaves the element name — a
; tag name is not a symbol — alone.
(namespace_selector
  .
  (tag_name) @reference.identifier)

; `@import "other.css";` — the engine strips the quotes from an import path.
(import_statement
  (string_value) @import.path) @import

; `@import url("theme.css");` and `@import url(theme.css);`
(import_statement
  (call_expression
    (arguments
      [(string_value) (plain_value)] @import.path))) @import

; `@use "buttons";` and `@forward "buttons";` — the Sass module system. The path is
; what the import names; the `as <namespace>` clause binds a local name for it, and
; a namespaced `@include ns.mixin()` resolves through that name.
(use_statement
  (string_value) @import.path) @import

(use_statement
  alias: (identifier) @name) @definition.module

(forward_statement
  (string_value) @import.path) @import
