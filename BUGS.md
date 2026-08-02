# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every
stage.

Format: `- [ ] B<N>: <symptom> — <where> — <status/notes>`

Every open entry below is a *characterised limitation* rather than breakage: the
behaviour is reported to the user, and no operation silently does the wrong thing.

## Open

- [ ] B5: `find_unused` follows resolved call edges only. Code reached exclusively
  through a trait object or interface value, a function held in a map or struct
  field, or a name assembled at runtime is live code with nothing in the workspace to
  distinguish it from dead code, and is still listed. A symbol used only from a file
  that failed to parse is the same. Two former halves are fixed: a name spelled in any
  string literal is now excluded (reflection and handler tables), and a reference
  cycle nothing outside reaches is now reported as a dead group.
- [ ] B10: Helm values precedence is now decided among the chart files a workspace
  scan can see — a subchart's `values.yaml` loses to its parent's, and the winner is
  marked. What stays invisible is the command line: whether a `values-*.yaml` is
  passed with `-f` at all, the order of several `-f` files, and every `--set`. Those
  cases are reported undecided, each naming the input that would decide it.
- [ ] B11: three SCSS forms fail under `tree-sitter-scss`, each refused rather than
  mis-handled: empty parentheses on a declaration (`@mixin m()`), empty parentheses on
  a call (`@include m();`), and a namespaced include after `@use 'x' as t`
  (`@include t.m(…)`). Fixing these is upstream grammar work.
- [ ] B12: Terraform loses the third and later step past an index traversal —
  `x.y[0].z.w` keeps `z` and `w`, `x.y[0].z.w.q` loses `q`. Each step needs its own
  query pattern. The address and the first two attribute reads survive.

## Fixed

- [x] B0a: `LineIndex` invented a phantom trailing line for files ending in a newline,
  so `"a\nb\n"` counted 3 lines and an EOF offset reported a column past the last
  character — `src/span.rs`. Fixed: a trailing newline terminates the final line;
  columns clamp to the line end.
- [x] B0b: `.gitignore` was ignored outside a git repository, so scans of worktrees
  and exported trees walked `target/`, `node_modules/` etc — `src/scan.rs`. Fixed with
  `WalkBuilder::require_git(false)`.
- [x] B1: SCSS was parsed with the plain CSS grammar, so `$variables`, `@mixin`,
  `@include` and `@use` were all parse errors. Fixed at the root by adding the
  `tree-sitter-scss` grammar. A test asserts the CSS grammar still rejects SCSS
  syntax, so the split is real rather than cosmetic.
- [x] B2: a Helm template action in a structural position yielded a YAML tree
  reflecting no single rendering. Fixed for the analyses that reason about values: a
  key wrapped in `{{- if }}` now produces a stop naming the exact condition, and the
  condition's own `.Values` key resolves. Masking itself is unchanged by design — it
  is what keeps byte offsets valid — so the symbol index still shows guarded keys
  unconditionally; only provenance and stitch consult the guards.
- [x] B3: deleting a CSS selector left its `{ ... }` block orphaned. The delete widens
  the selector's span to the whole rule when it is alone on it, or to that selector
  and its comma when the rule has others.
- [x] B4: import liveness was name-based, so anything a language brings into scope
  invisibly looked unused. Per-language guards now hold back and report: Python
  `__future__` imports, `__all__` re-exports and dotted registration imports;
  TypeScript type-only imports, JSDoc `{Foo}` mentions, JSX pragmas and `typeof X`;
  Go blank imports and packages whose clause name cannot be derived from the path.
  Zig was verified to need none. Two real false positives fell out of it: Python
  `import a.b` binds `a`, not `b`, and `gopkg.in/yaml.v2` binds `yaml`, not `v2`.
- [x] B6: consecutive standalone Go `import "x"` lines were not sorted, because the
  `import` keyword sits outside the `import_spec` span and looked like unrelated code
  ending the block.
- [x] B7: Helm `.Values` references lived inside masked actions and were invisible.
  Fixed by parsing the actions: paths resolve through pipelines, function arguments,
  `with` scopes, `$.` and into `define` bodies reached by `include`. Fields of a dot
  bound by `range`, values reached via `index .Values "a-b"`, and computed template
  names are named as unresolved rather than resolved.
- [x] B8: Terraform splat traversals lost their trailing segments. `[*].id` and
  `.*.id` now capture every following attribute; B12 records what an index traversal
  still loses.
- [x] B9: `.tfvars` top-level attributes now produce `Key` symbols, so values files
  are in the index rather than needing provenance to walk the tree itself.
