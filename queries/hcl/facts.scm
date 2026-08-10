; Terraform / HCL fact extraction.
; Capture conventions are documented in src/extract.rs.
;
; HCL has no lexical bindings: every Terraform declaration is addressed by the
; string labels on its block (`resource "aws_s3_bucket" "main"` is the address
; `aws_s3_bucket.main`), and every use site is a scope-traversal expression whose
; first segment names the namespace. So the two jobs here are: pull the label
; text out of its quotes, and pick the one segment of each traversal that a
; rename has to rewrite.
;
; A label is a `string_lit` whose text includes the quote characters; its inner
; `template_literal` child is the bare content. Names are always captured on the
; `template_literal`, because @name is the byte range a rename replaces and it
; must not swallow the quotes.

; ---------------------------------------------------------------- scopes
(config_file) @scope
(body) @scope
(object) @scope
(for_expr) @scope

; ------------------------------------------------------------ definitions
; Block patterns are distinguished by label arity, which is structural rather
; than semantic: the anchors below require the labels and the opening brace to be
; consecutive named siblings, so a one-label pattern cannot match a two-label
; block and vice versa.

; No labels: `terraform {}`, `locals {}`, `lifecycle {}`. The block type keyword
; is the only name such a block has.
(block
  . (identifier) @name
  . (block_start)) @definition.block

; One label. `variable` and `module` get dedicated kinds because they are the
; addressable declarations Terraform resolves by bare name (`var.x`, `module.x`).
(block
  . (identifier) @_kw
  . (string_lit (template_literal) @name)
  . (block_start)
  (#eq? @_kw "variable")) @definition.variable

(block
  . (identifier) @_kw
  . (string_lit (template_literal) @name)
  . (block_start)
  (#eq? @_kw "module")) @definition.module

; Every other one-label block — `output`, `provider`, `dynamic`, provider-defined
; block types — is a Block. `output` is deliberately here and not a Variable: an
; output is not substituted into expressions in its own module, it is a boundary
; declaration consumed as `module.<name>.<output>` from outside.
(block
  . (identifier) @_kw
  . (string_lit (template_literal) @name)
  . (block_start)
  (#not-any-of? @_kw "variable" "module")) @definition.block

; Two labels: `resource "type" "name"` and `data "type" "name"`. The Terraform
; address is `type.name`, and a query cannot synthesise a compound name, so the
; second label is the renameable @name and the first qualifies it as a container.
; The symbol's qualified name is therefore `type::name` — the address with the
; engine's separator and not Terraform's dot.
(block
  . (identifier)
  . (string_lit (template_literal) @container.name)
  . (string_lit (template_literal) @name)
  . (block_start)) @definition.block @container

; `locals { x = ... }` — each attribute declares the address `local.x`. Only
; attributes of a `locals` block are definitions; arguments of other blocks are
; provider-defined settings, not renameable declarations.
(block
  . (identifier) @_kw
  (body
    (attribute
      . (identifier) @name) @definition.variable)
  (#eq? @_kw "locals"))

; A top-level attribute in a `.tfvars` file assigns a root-module variable. The
; grammar is shared with `.tf`, where a bare top-level attribute would be invalid,
; so this pattern only ever fires on values files. Capturing them puts values files
; in the index instead of leaving provenance to walk the tree itself.
(config_file
  (body
    (attribute
      . (identifier) @name) @definition.key))

; ------------------------------------------------------------- references
; A traversal is a `variable_expr` root followed by `.step` `get_attr` children,
; flat under one `expression`. Which segment is the renameable one depends on the
; root namespace, so every form is matched explicitly — a catch-all over
; `get_attr` would label `var.x`'s `x` a field access and lose that distinction.

; `var.NAME`, `local.NAME`, `module.NAME`: the second segment names the
; declaration. This is the reference form rename depends on most.
(expression
  . (variable_expr (identifier) @_ns)
  . (get_attr (identifier) @reference.identifier)
  (#any-of? @_ns "var" "local" "module"))

; `data.TYPE.NAME`: the third segment is the data block's name label, the second
; its type label.
(expression
  . (variable_expr (identifier) @_ns)
  . (get_attr (identifier) @reference.type)
  . (get_attr (identifier) @reference.identifier)
  (#eq? @_ns "data"))

; `TYPE.NAME`: a managed resource address, i.e. any traversal whose root is not
; one of Terraform's reserved namespaces.
(expression
  . (variable_expr (identifier) @reference.type)
  . (get_attr (identifier) @reference.identifier)
  (#not-any-of? @reference.type
    "var" "local" "module" "data" "each" "count" "self" "path" "terraform"))

; `each.value`, `count.index`, `self.attr`, `path.module`, `terraform.workspace`
; — evaluation-context values with no declaration site anywhere to rename.
(expression
  . (variable_expr (identifier) @_ns)
  . (get_attr (identifier) @reference.field)
  (#any-of? @_ns "each" "count" "self" "path" "terraform"))

; Attribute read on a two-segment address: the `.arn` of `aws_s3_bucket.main.arn`
; or the `.vpc_id` of `module.network.vpc_id`.
(expression
  . (variable_expr (identifier) @_ns)
  . (get_attr)
  . (get_attr (identifier) @reference.field)
  (#not-eq? @_ns "data"))

; `data.TYPE.NAME.attr` — one segment longer, so the attribute read sits a step
; further along.
(expression
  . (variable_expr (identifier) @_ns)
  . (get_attr)
  . (get_attr)
  . (get_attr (identifier) @reference.field)
  (#eq? @_ns "data"))

; A splat (`aws_instance.web[*].id`, `aws_instance.web.*.id`) does not continue the
; flat `get_attr` run: the grammar hangs everything after the `[*]` off a
; `splat` → `full_splat`/`attr_splat` node. Those trailing steps are attribute
; reads on each element, so they are fields, exactly as they would be without the
; splat. Matching inside the splat node and not as siblings is what makes them
; visible at all.
(splat
  (full_splat
    (get_attr (identifier) @reference.field)))

(splat
  (attr_splat
    (get_attr (identifier) @reference.field)))

; An index (`x.y[0].z`) leaves the following steps as flat `get_attr` siblings, but
; the `index` node sits between them and the traversal root, so the anchored
; patterns above stop at it. Anchoring to the index instead picks the run back up.
;
; Each step needs its own pattern, because a query cannot say "every sibling after
; this one". Six are written out: `x.y[0].a.b.c.d.e.f` is already far past anything
; Terraform expresses, and the depth is asserted by a test so the bound is a decision
; instead of an accident.
(expression
  (index)
  . (get_attr (identifier) @reference.field))

(expression
  (index)
  . (get_attr)
  . (get_attr (identifier) @reference.field))

(expression
  (index)
  . (get_attr)
  . (get_attr)
  . (get_attr (identifier) @reference.field))

(expression
  (index)
  . (get_attr)
  . (get_attr)
  . (get_attr)
  . (get_attr (identifier) @reference.field))

(expression
  (index)
  . (get_attr)
  . (get_attr)
  . (get_attr)
  . (get_attr)
  . (get_attr (identifier) @reference.field))

(expression
  (index)
  . (get_attr)
  . (get_attr)
  . (get_attr)
  . (get_attr)
  . (get_attr)
  . (get_attr (identifier) @reference.field))


(function_call
  . (identifier) @reference.call)

; ---------------------------------------------------------------- imports
; `source` on a module block is Terraform's only import: it names another
; configuration whose variables and outputs become this module's call surface.
; The module label is the local binding that surface is reached through.
(block
  . (identifier) @_kw
  . (string_lit (template_literal) @import.alias)
  . (block_start)
  (body
    (attribute
      . (identifier) @_attr
      (expression (literal_value (string_lit (template_literal) @import.path)))))
  (#eq? @_kw "module")
  (#eq? @_attr "source")) @import
