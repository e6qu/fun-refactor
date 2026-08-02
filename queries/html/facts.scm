; HTML fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; HTML defines almost nothing of its own: its value to the index is that it is the
; consumer side of CSS. `id="x"` defines the element id that CSS `#x` and any
; `href="#x"`/`for="x"` point at, and `class="btn"` is a use site of the CSS class
; `.btn`. Element and attribute names are markup vocabulary, not user-chosen
; identifiers, so they are never symbols.
;
; Name spans: the grammar exposes `attribute_value` inside `quoted_attribute_value`,
; so a captured name/reference span covers the value only — the quotes stay outside
; and a rename never touches them. Unquoted values (`class=bare`) are the same
; `attribute_value` node without a wrapper, so both forms are handled.
;
; KNOWN GAP — multi-valued class attributes: `class="page dark"` is a single
; `attribute_value` node, so the reference span and name are the whole string
; `page dark`, not two references `page` and `dark`. Splitting on whitespace
; (and mapping each piece back to a sub-span) has to happen in the engine, after
; extraction; a tree-sitter query cannot subdivide a token. The same applies to
; multi-valued `headers=` and `aria-labelledby=`. Until then, matching CSS class
; definitions against HTML class references is exact only for single-class values.
;
; Attribute names are matched case-insensitively (`ID=`, `Class=` are legal HTML).

; ---------------------------------------------------------------- scopes
; Not lexical scoping — HTML has none — but the element tree is the containment
; structure that makes "which element is this fact in" answerable.
(document) @scope
(element) @scope
(script_element) @scope
(style_element) @scope

; ------------------------------------------------------------ definitions
; `id="root"` — the definition site of an element id.
(attribute
  (attribute_name) @_attr
  (quoted_attribute_value
    (attribute_value) @name)
  (#match? @_attr "^(?i)id$")) @definition.element-id

(attribute
  (attribute_name) @_attr
  (attribute_value) @name
  (#match? @_attr "^(?i)id$")) @definition.element-id

; ------------------------------------------------------------- references
; `class="btn"` — a use site of a CSS class definition. See the multi-value gap
; above: the span is the whole attribute value.
(attribute
  (attribute_name) @_attr
  (quoted_attribute_value
    (attribute_value) @reference.string)
  (#match? @_attr "^(?i)class$"))

(attribute
  (attribute_name) @_attr
  (attribute_value) @reference.string
  (#match? @_attr "^(?i)class$"))

; Attributes whose value is an element id: `for=`, and the ARIA relations.
(attribute
  (attribute_name) @_attr
  (quoted_attribute_value
    (attribute_value) @reference.string)
  (#match? @_attr "^(?i)(for|form|list|headers|aria-labelledby|aria-describedby|aria-controls|aria-owns)$"))

(attribute
  (attribute_name) @_attr
  (attribute_value) @reference.string
  (#match? @_attr "^(?i)(for|form|list|headers|aria-labelledby|aria-describedby|aria-controls|aria-owns)$"))

; `href="#sec"` — an in-document anchor. The `#` is part of the captured span
; because the grammar gives no node for the fragment alone; stripping it is the
; engine's job when it resolves the reference to an id. Fragment-bearing hrefs
; that also name a file (`other.html#sec`) are deliberately not matched: that is
; a cross-document link, and the fragment is not resolvable here.
(attribute
  (attribute_name) @_attr
  (quoted_attribute_value
    (attribute_value) @reference.string)
  (#match? @_attr "^(?i)href$")
  (#match? @reference.string "^#."))

; ---------------------------------------------------------------- imports
; `<link href="theme.css">` — the CSS file this document depends on. This is the
; edge that makes a class rename cross the language boundary.
(element
  (start_tag
    (tag_name) @_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value
        (attribute_value) @import.path))
    (#match? @_tag "^(?i)link$")
    (#match? @_attr "^(?i)href$")) @import)

; `<script src="app.js">`
(script_element
  (start_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value
        (attribute_value) @import.path))
    (#match? @_attr "^(?i)src$")) @import)
