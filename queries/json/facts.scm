; JSON fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; A JSON document is a tree of keys, and the key path *is* the API, exactly as
; it is in a values file. So every member key is a definition and containment
; supplies the path: the engine's qualifier holds one level (`image::tag`) and
; the full path is walked through `Symbol::container`, which the engine links by
; span containment.
;
; JSON has no anchors, no aliases and no comments, so there is no intra-file
; reference edge at all. A reference into a JSON document comes from outside it,
; from an HCL `.tf.json` neighbour or from code reading a key by name, and those
; are edges the stitching pass draws and not this file.

(document) @scope
(object) @scope
(array) @scope

; A key whose value is a collection qualifies the keys nested under it, so `tag`
; under `image` reports as `image::tag`. The container is the *value* node and
; not the pair: a pair contains its own key, which would make every key qualify
; itself.
(pair
  key: (string (string_content) @container.name)
  value: [(object) (array)] @container)

; Every member key. The name is the string's content and not the string, so a
; rename rewrites the text between the quotes and leaves the quotes alone. JSON
; has no unquoted key, so there is one shape here and not YAML's three.
(pair
  key: (string (string_content) @name)) @definition.key
