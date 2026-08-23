; YAML fact extraction, shared by plain YAML and Helm templates.
; Capture conventions are documented in src/extract.rs.
;
; In a values file the key path *is* the API, so every mapping key is a
; definition and containment supplies the path. The engine's qualifier holds one
; level (`image::tag`); the full path is walked through `Symbol::container`,
; which the engine links by span containment.
;
; Anchors and aliases are the one genuine intra-file reference edge YAML has, and
; the only rename that can be verified without leaving the file. Note that YAML
; discards anchors during composition, so this pre-composition view is strictly
; more information than a loaded document carries.
;
; Helm: `{{ ... }}` actions are masked to equal-length bytes before parsing (see
; src/parse.rs), so template contents are invisible to these patterns by
; construction. That is deliberate — a masked action leaves a well-formed YAML
; skeleton, and the surrounding keys extract normally. Resolving `.Values.x`
; needs a separate template-aware pass over `Parsed::template_actions`; nothing
; below attempts it. The one shape masking cannot rescue is an action in *key*
; position (`{{ .Values.k }}: v`): the key has no name before the template
; renders, so it is blanked and matches nothing here. Whether that also fails
; the parse depends on what surrounds it, which is why the file carries
; `FactGap::TemplatedKeys` instead of leaving the report to `has_errors`.

(stream) @scope
(document) @scope
(block_mapping) @scope
(block_sequence) @scope
(flow_mapping) @scope
(flow_sequence) @scope

; A key whose value is a collection qualifies the keys nested under it, so `tag`
; under `image` reports as `image::tag`. The container is the *value* node, not
; the pair: a pair contains its own key, which would make every key qualify
; itself.
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @container.name))
  value: (block_node) @container)

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @container.name))
  value: (flow_node [(flow_mapping) (flow_sequence)] @container))

; An anchor on a scalar value is the one definition a scalar can hold, so scalar
; values qualify it the same way collection values qualify their keys. Without
; this, `k: &a 1` would report an unqualified anchor while `k: &a {}` reported a
; qualified one.
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @container.name))
  value: (flow_node (anchor)) @container)

; `<<` is the merge key: it names no field, it splices the aliased mapping in.
; The alias on its right is captured as a reference below, which is the whole of
; what a merge means for rename.
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @name))
  (#not-eq? @name "<<")) @definition.key

(flow_pair
  key: (flow_node (plain_scalar (string_scalar) @name))
  (#not-eq? @name "<<")) @definition.key

; Quoted keys. The grammar gives quoted scalars no inner-content node, so @name
; necessarily spans the quotes as well — a rename of `"old key"` rewrites the
; quotes along with the text, which is correct but means the reported name is
; not bare like a plain key's.
(block_mapping_pair
  key: (flow_node [(double_quote_scalar) (single_quote_scalar)] @name)) @definition.key

(flow_pair
  key: (flow_node [(double_quote_scalar) (single_quote_scalar)] @name)) @definition.key

; Anchors. `&name` binds the node that follows it, and the definition spans that
; whole node — an inline-anchor refactoring needs the value to substitute, not
; just the label.
(block_node
  (anchor (anchor_name) @name)) @definition.anchor

(flow_node
  (anchor (anchor_name) @name)) @definition.anchor

; `*name`, including the alias half of a `<<:` merge — the other end of the
; anchor rename pair.
(alias
  (alias_name) @reference.identifier)
