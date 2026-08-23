; XML fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; What is renameable in XML is the id/idref graph and the namespace prefixes.
; Element and attribute names are document vocabulary, not user-chosen names, so
; they are not symbols; the one exception is a namespace *prefix*, which is bound
; by `xmlns:` and used on every prefixed name.
;
; KNOWN GAP — attribute values include their quotes. tree-sitter-xml builds an
; `AttValue` node whose only children are the two quote characters and any entity
; references; the text between them is matched character-by-character by an
; anonymous rule and never becomes a node (see common/common.mjs `att_value`).
; So the narrowest capturable span for `id="a"` is `"a"` *with* the quotes, and
; `Symbol::name` is therefore `"a"` (quoted), not `a`. Trimming the quotes has to
; happen in the engine — the same trimming it already applies to import paths —
; before a rename can rewrite these spans. tests/facts_xml.rs pins the current
; behaviour so the fix is a visible change.
;
; KNOWN GAP — a prefixed name is one token. `xmlns:foo` and `foo:child` are single
; `Name` nodes, so the captured span covers the whole thing, prefix *and* local
; part. Renaming the prefix `foo` means rewriting the leading segment of these
; spans, which again is engine-side post-processing.

; Namespace prefixes are scoped to the element that declares them, so the element
; tree is a real scope tree here, not just a convenience.
(document) @scope
(element) @scope

; `id="a"` and `xml:id="a"` define an element id. XML is case-sensitive, so the
; attribute names are matched exactly. A DTD may declare an ID-typed attribute
; under any other name; those are not recognised without reading the DTD.
(Attribute
  (Name) @_attr
  (AttValue) @name
  (#any-of? @_attr "id" "xml:id")) @definition.element-id

; `xmlns:foo="urn:foo"` binds the prefix `foo`. The name span is the whole
; `xmlns:foo` attribute name (see the gap note above). A default declaration
; `xmlns="..."` binds no prefix and so defines no renameable name.
(Attribute
  (Name) @name
  (#match? @name "^xmlns:")) @definition.module

; `idref`/`idrefs`/`ref` point at an element id.
(Attribute
  (Name) @_attr
  (AttValue) @reference.string
  (#any-of? @_attr "idref" "idrefs" "ref"))

; `href="#a"` and `href="other.xml#a"` — a link to an element id. The quotes are
; inside the span, and the extractor narrows what is left to the fragment; an
; absolute URL is left alone there, since its fragment names another document.
(Attribute
  (Name) @_attr
  (AttValue) @reference.string
  (#eq? @_attr "href")
  (#match? @reference.string "#."))

; Any prefixed name — `<foo:child>`, `</foo:child>`, `foo:attr="v"` — uses a
; prefix bound by an `xmlns:` declaration. `xmlns:` declarations are definitions,
; not uses, and the `xml:` prefix is built in, so both are excluded.
((Name) @reference.identifier
  (#match? @reference.identifier "^[^:]+:")
  (#not-match? @reference.identifier "^xmlns:")
  (#not-match? @reference.identifier "^xml:"))

; An internal-subset entity is XML's only binding form: `<!ENTITY brand "Acme">`
; declares a name that `&brand;` substitutes. Capturing both puts entities in the
; index, so rename and inline reach them like any other symbol.
(GEDecl
  (Name) @name) @definition.constant

(EntityRef
  (Name) @reference.identifier)
