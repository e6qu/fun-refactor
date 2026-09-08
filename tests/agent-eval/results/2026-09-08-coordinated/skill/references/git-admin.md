# Git staging and worktrees

## Staging and commits

Use `fr git stage PATH...` for explicit paths, inspect the returned basis and index changes, then use the same selection with `--basis TOKEN --write`.
Staging uses raw bytes and refuses unsupported index states and affected content filters.
Its `fr git stage-history` IDs belong to an index journal, not source history.
Use `fr git commit` help and preview for an authorized commit; the reviewed write binds the branch, parent and index.
Do not infer permission to commit, push or publish from a request to inspect or export.

## Worktrees

`fr git worktree list` inspects registrations.
`create PATH --branch NAME` previews a new branch; `--existing-branch NAME` attaches an unused local branch at its reviewed tip.
Creation writes require their returned `--basis TOKEN --write` and support only the documented raw checkout subset.
Existing-branch creation retains the ref and its configuration. Preview refusals expose unsupported layouts or state.

Use `recover PATH` only for an owned incomplete creation, and `remove PATH` for reviewed removal of an owned clean worktree.
Removal retains the branch and returns a `removal_record` archive path.
Use `resume-removal RECORD` to inspect or finish incomplete removal with a fresh basis.
Use `compact-removal RECORD` only to discard a completed recovery record after reviewing its basis; it retains an audit summary and cannot be undone through `fr`.
Keep using the original record path after compaction. Worktree undo/redo and automatic archive retention remain unavailable.

Each operation has its own basis. Do not substitute source revisions, transaction IDs or tokens from another command.
Only `applied: true` confirms a Git mutation. Some partial or uncertain outcomes exit successfully with `applied: null` and a recovery path.
Inspect that state before retrying, preserve unknown files and locks, and use the matching recovery command.
