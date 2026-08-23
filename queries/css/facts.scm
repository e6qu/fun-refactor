; CSS fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; This file serves Language::Css only. SCSS has its own grammar and its own query
; file, queries/scss/facts.scm, which repeats every pattern below and adds the
; Sass-only ones (`$vars`, `@mixin`/`@include`, `@function`, `@use`/`@forward`).
; The two are meant to stay in step: a change here that is not SCSS-specific
; belongs there too.
;
; Naming notes (asserted in the tests):
;   * `.btn` -> name is `btn`; the leading `.` is outside the name span.
;   * `#main` -> name is `main`; the leading `#` is outside the name span.
;   * `--brand-color` -> name is `--brand-color`, dashes included: that is the
;     spelling used at both the definition and the `var()` use site, so a rename
;     rewrites matching text on both sides.
;
; Every selector occurrence is a definition site. CSS has no single canonical
; definition of a class: `.btn { }` written twice declares the same class twice,
; and renaming it must rewrite all of them.

; CSS scoping is not lexical, but block structure is what nests declarations
; inside rules, media queries and keyframes, so it is the useful scope tree.
(stylesheet) @scope
(block) @scope
(keyframe_block_list) @scope

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

; `@namespace svg url(...)` binds a namespace prefix usable in selectors.
(namespace_statement
  (namespace_name) @name) @definition.module

; `var(--brand-color)` and `var(--brand-color, fallback)`: only arguments spelled
; like a custom property are references; the fallback value is not.
(call_expression
  (function_name) @_fn
  (arguments
    (plain_value) @reference.identifier)
  (#eq? @_fn "var")
  (#match? @reference.identifier "^--"))

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
