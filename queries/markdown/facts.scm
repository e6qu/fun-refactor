; Markdown fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Grammar: tree-sitter-md-025, which parses a document with TWO grammars. The block
; grammar gives headings, link reference definitions, code fences and tables, and
; leaves the text of every paragraph, heading and table cell as an opaque `inline`
; node. The inline grammar parses those nodes, and that is where links, link text,
; labels and destinations live. src/parse.rs runs both passes and hands the extractor
; the block tree plus one sub-tree per inline node, all indexing the same bytes.
;
; So this file has two halves, split by the marker below: everything above it is
; compiled against the block grammar, everything below against the inline grammar.
; A query only compiles against the grammar whose node names it uses, which is why
; the split is necessary and why the halves may not be reordered.
;
; What a Markdown rename touches: a heading and every `#slug` link that points at
; it, and a link reference definition and every `[text][label]` that uses it.
;
; `link_destination` is one node, so the extractor narrows a fragment-bearing one to
; the fragment: `[x](#intro)` and `[x](guide.md#intro)` both name `intro`.
;
; NOT AVAILABLE — footnotes. This grammar has no footnote rule either: `[^fn]: text`
; parses as a paragraph whose inline content is the shortcut link `[^fn]`. Both the
; definition and every use surface as `@reference.string` named `^fn`, which still
; lets a rename find all of the occurrences, but there is no LinkDef symbol for a
; footnote.

; The block grammar nests `section` nodes under their headings, but every Markdown
; name is document-global — an anchor resolves against the whole file — so the
; document stays the only scope.
(document) @scope

; `# Title` — the heading text is the name because that is what the anchor slug is
; generated from. The opening `#` markers are outside `heading_content`; an optional
; closing marker (`## Title ##`) is not, and the extractor trims it.
(atx_heading
  heading_content: (inline) @name) @definition.heading

; `Title` followed by `=====` or `-----`. The content is a paragraph wrapping the
; inline text; the inline node is the tighter span, so it is the name.
(setext_heading
  heading_content: (paragraph
    (inline) @name)) @definition.heading

; `[label]: http://example.com` — the label is the name. `link_label` includes its
; brackets, which the extractor trims: renaming over them would write `new: /a`.
(link_reference_definition
  (link_label) @name) @definition.link-def

; ```rust — the language tag of a fenced code block names a language, which is
; useful when tracking which files embed which language. `info_string` may carry
; further words (```rust,ignore), so the `language` child is the reference.
(fenced_code_block
  (info_string
    (language) @reference.identifier))

; ==== inline grammar ====
;
; Patterns below this marker run against the inline sub-trees. Their spans are
; offsets into the original document, exactly like the block patterns above.

; `[text][label]` — a full reference link points at a link reference definition.
(full_reference_link
  (link_label) @reference.string)

; `[label][]` — a collapsed reference link, where the text *is* the label.
(collapsed_reference_link
  (link_text) @reference.string)

; `[label]` — a shortcut reference link, same again with the trailing `[]` dropped.
(shortcut_link
  (link_text) @reference.string)

; `[text](#anchor)` and `[text](guide.md#anchor)` — a link to a heading anchor. The
; destination is one node, so the extractor narrows the span to the fragment; an
; absolute URL is left alone there, since its fragment names another document's
; heading.
(inline_link
  (link_destination) @reference.string
  (#match? @reference.string "#."))

; `![alt][label]` — a reference image uses a link reference definition just as a
; reference link does, so renaming the definition has to rewrite it too.
(image
  (link_label) @reference.string)
