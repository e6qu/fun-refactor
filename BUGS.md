# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every stage.

Format: `- [ ] B<N>: <symptom> — <where> — <status/notes>`

## Open

- [ ] B1: SCSS is parsed with the plain CSS grammar, so SCSS-only syntax (`$vars`,
  `@mixin`, nesting) is reported as parse errors — `src/parse.rs`. The grammar set
  inherited from funveil has no SCSS grammar. Surfaced honestly (never silently
  mis-parsed) and covered by a test that asserts the errors appear. Fix requires adding
  an SCSS grammar; blocks full SCSS support in Stages 2/5/6.
- [ ] B2: Helm template masking makes `{{ ... }}` invisible to the YAML grammar, so a
  template action occupying a structural position (e.g. a whole `{{- if }}` block
  wrapping map keys) yields a YAML tree that does not reflect any single rendering —
  `src/parse.rs`. Acceptable for reference/provenance work, but render-dependent
  structure must be reported as unresolved rather than guessed (D5).

## Fixed

- [x] B0a: `LineIndex` invented a phantom trailing line for files ending in a newline,
  so `"a\nb\n"` counted 3 lines and an EOF offset reported a column past the last
  character — `src/span.rs`. Fixed: a trailing newline terminates the final line;
  columns clamp to the line end. Regression tests added.
- [x] B0b: `.gitignore` was ignored outside a git repository, so scans of worktrees
  and exported trees walked `target/`, `node_modules/` etc — `src/scan.rs`. Fixed with
  `WalkBuilder::require_git(false)`.
