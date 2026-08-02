; Markdown fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; Grammar: tree-sitter-markdown-fork. Unlike upstream tree-sitter-markdown, this
; fork parses block *and* inline structure into one tree — `link`, `link_text`,
; `link_label` and `link_destination` are all present without a second inline
; parser — so anchor links and reference links are reachable from these queries.
;
; What a Markdown rename touches: a heading and every `#slug` link that points at
; it, and a link reference definition and every `[text][label]` that uses it.
;
; KNOWN GAP — ATX heading names carry their padding. The grammar's
; `heading_content` node starts after the `#` markers but *includes* the spaces
; around the text: `# Title` yields the name ` Title`, and `#   T   #` yields
; `   T   ` (the closing marker is excluded). Setext headings have no such
; padding, so `Title\n=====` yields exactly `Title`. Trimming is engine-side
; post-processing; tests/facts_markdown.rs pins both behaviours.
;
; KNOWN GAP — an anchor destination keeps its `#`. `[x](#intro)` yields the
; reference name `#intro`, because `link_destination` is one node.
;
; NOT AVAILABLE — footnotes. `[^fn]: text` parses as an ordinary paragraph whose
; first child is a shortcut link `[^fn]`, so there is no footnote-definition node
; to make a symbol from. Both the definition and every use surface as
; `@reference.string` named `^fn`, which still lets a rename find all of the
; occurrences, but there is no LinkDef symbol for a footnote.

; ---------------------------------------------------------------- scopes
; Markdown blocks are a flat list in this grammar — there is no `section` node
; nesting content under its heading — so the document is the only scope.
(document) @scope

; ------------------------------------------------------------ definitions
; `# Title` — the heading text is the name because that is what the anchor slug
; is generated from. The `#` markers are outside the name span.
(atx_heading
  (heading_content) @name) @definition.heading

; `Title` followed by `=====` or `-----`.
(setext_heading
  (heading_content) @name) @definition.heading

; `[label]: http://example.com` — the label is the name; the brackets and the
; destination are outside the name span.
(link_reference_definition
  (link_label) @name) @definition.link-def

; ------------------------------------------------------------- references
; `[text][label]` — a full reference link points at a link reference definition.
(link
  (link_label) @reference.string)

; `[label]` and `[label][]` — shortcut and collapsed reference links, where the
; label *is* the link text. The anchors make this pattern match only when
; `link_text` is the link's sole named child, which excludes `[text][label]`
; and `[text](dest)`.
(link
  .
  (link_text) @reference.string
  .)

; `[text](#anchor)` — an in-document link to a heading anchor. A destination
; that names a file is a cross-document link and is not matched.
(link
  (link_destination) @reference.string
  (#match? @reference.string "^#."))

; ```rust — the language tag of a fenced code block names a language, which is
; useful when tracking which files embed which language.
(fenced_code_block
  (info_string) @reference.identifier)
