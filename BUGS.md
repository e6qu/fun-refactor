# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every
stage.

Format: `- [ ] B<N>: <symptom> — <where> — <status/notes>`

Every open entry below is a *characterised limitation* rather than breakage: the
behaviour is reported to the user, and no operation silently does the wrong thing.

## Open

- [ ] B2: Helm template masking makes `{{ ... }}` invisible to the YAML grammar, so a
  template action occupying a structural position (e.g. a whole `{{- if }}` block
  wrapping map keys) yields a YAML tree that does not reflect any single rendering —
  `src/parse.rs`. Acceptable for reference/provenance work, but render-dependent
  structure must be reported as unresolved rather than guessed (D5).
- [ ] B4: Organize-imports decides liveness by name, so a Python module imported for
  a registration side effect, or a TypeScript type used only in a JSDoc comment,
  looks unused. Rust trait-shaped (upper-camel-case) bindings are held back for this
  reason; the equivalent guard does not exist for other languages.
- [ ] B5: `find_unused` follows resolved call edges only, so dynamic dispatch,
  reflection and string-keyed handler tables can put live code on the list, and
  mutual recursion can hide dead code from it. Stated in the command output.
- [ ] B7: Helm `.Values` references live inside masked template actions and are
  therefore invisible to the YAML queries. Resolving them needs a template-aware pass
  over `Parsed::template_actions`; provenance reports them as render-dependent rather
  than guessing.
- [ ] B8: Terraform `count`/`for_each`/splat traversals lose their trailing segments
  (`aws_instance.web[*].id` yields the address but not `id`) — the grammar puts those
  steps under `splat`/`index` rather than as flat `get_attr` siblings. The renameable
  part survives; the attribute read is lost.
- [ ] B10: Helm values competitions are never *decided*: `--set` and `-f` ordering
  happen at the command line and are invisible to a workspace scan, so two candidate
  sources are reported with no winner and a `PrecedenceUndetermined` stop.
- [ ] B11: three SCSS forms still fail under `tree-sitter-scss`, each refused rather
  than mis-handled: empty parentheses on a declaration (`@mixin m()`), empty
  parentheses on a call (`@include m();`), and a namespaced include after
  `@use 'x' as t` (`@include t.m(…)`). A narrower successor to the old B1.

## Fixed

- [x] B0a: `LineIndex` invented a phantom trailing line for files ending in a newline,
  so `"a\nb\n"` counted 3 lines and an EOF offset reported a column past the last
  character — `src/span.rs`. Fixed: a trailing newline terminates the final line;
  columns clamp to the line end. Regression tests added.
- [x] B0b: `.gitignore` was ignored outside a git repository, so scans of worktrees
  and exported trees walked `target/`, `node_modules/` etc — `src/scan.rs`. Fixed with
  `WalkBuilder::require_git(false)`.
- [x] B1: SCSS was parsed with the plain CSS grammar, so `$variables`, `@mixin`,
  `@include` and `@use` were all parse errors. Fixed at the root by adding the
  `tree-sitter-scss` grammar and routing `Language::Scss` to it. CSS and SCSS are
  genuinely different languages and now get different grammars; a test asserts the
  CSS grammar still rejects SCSS syntax, so the split is real rather than cosmetic.
- [x] B3: Deleting a CSS selector left its `{ ... }` block orphaned. A selector's
  `full_span` is the selector — which is what a rename needs — so the *delete* widens
  it instead: the whole rule when the selector is alone on it, or just that selector
  and its comma when the rule has others.
- [x] B6: Consecutive standalone Go `import "x"` lines were not sorted, because the
  `import` keyword sits outside the `import_spec` span and so looked like unrelated
  code ending the block. A statement now owns its line if only its own introducing
  keyword precedes it.
- [x] B9: `.tfvars` top-level attributes now produce `Key` symbols, so values files
  are in the index rather than needing provenance to walk the tree itself.
