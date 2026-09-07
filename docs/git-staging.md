# Git staging

```sh
fr git stage src/main.rs src/api.rs
fr git stage src/main.rs src/api.rs --basis TOKEN
fr git stage src/main.rs src/api.rs --basis TOKEN --write
```

`fr git stage` previews proposed index entries and prints JSON in both output modes.
The default reports `operation: "stage-preview"`, `applied: false` and `durability: null`, without index or object writes.
On Unix, `--basis TOKEN --write` applies the reviewed entries and reports `operation: "stage-apply"` and `applied: true`.
`--write` requires a basis. `write_supported` reports host support; writes currently require Unix lock identity checks.
Working files and HEAD remain unchanged. This command does not create a source-history transaction or commit.

## Selection and proposed entries

Provide one through 32 literal repository-relative file arguments. Paths normalize, deduplicate and sort before reporting.
Argument order and duplicates do not affect the basis. Use `--` before a path beginning with a dash.
Nested `-C` directories still select repository-relative paths. Linked worktrees select their own index and working files.
Directories, absolute paths, parent traversal, symlink traversal, selected index symlinks and submodules cause refusal.
A selected unmerged index entry also causes refusal. Unrelated conflicts do not block preview or application.

Actions compare stage-zero index entries with proposed working-file entries. HEAD does not participate, so unborn repositories work too.
Each row contains `path`, `action`, `before`, `after` and nullable `working_bytes`.
Present entries contain `oid` and `mode`; absent entries are null.

| Action | Selected index entry | Working file |
|---|---|---|
| `add` | Absent | Present |
| `remove` | Present | Absent |
| `update` | Present | Different raw content or projected mode |
| `unchanged` | Present | Same content identity and projected mode |

A path absent from both states causes refusal.
Untracked regular text files can produce additions. Ignored untracked paths cause refusal; tracked paths remain eligible even when ignore patterns match them.
Present working files must contain UTF-8 text without NUL bytes. Removal previews use index metadata and do not need the old source bytes.
Source bodies stay outside the output. `counts` contains `paths`, `add`, `update`, `remove` and `unchanged`; action counts partition the selected paths.
The argument limit bounds rows, without bounding file sizes, path lengths or internal collection work.

## Raw staging semantics

`staging_semantics: "raw-bytes-owner-executable"` identifies the proposed representation.
The preview hashes captured working bytes with Git's blob hash, without content conversion or object writes.
Proposed object IDs need not exist in the object database yet. SHA-1 and SHA-256 repositories use their respective Git identities.
CRLF bytes remain CRLF even when attributes would normalize them during `git add`.

On Unix, proposed modes project the owner-executable bit to `100644` or `100755`, independently of `core.filemode`.
Other permission bits do not affect that projection. Hosts without Unix permissions propose `100644`.
A matching content hash with a different projected mode produces an update.
This proposal therefore describes the raw representation explicitly; it does not reproduce every Git conversion or configuration rule.

## Basis and drift checks

`basis` binds the canonical repository root, tool version, staging semantics and complete sorted entry result.
Pass it back as `--basis TOKEN` with the same selected paths to require that observation again.
Successful verification reports `basis_verified: true`; a query without a supplied token reports false.
A changed selected index identity, raw working body, existence state or projected mode causes refusal.
Unrelated staged and working changes do not invalidate this basis. Index flags outside the reported mode and object identity are not part of it.

Before emitting a result, the preview rechecks the selected index entries, rereads selected working files, and checks the index again.
All validation completes before any result appears. A validation failure emits an error without partial entries.
The checks do not freeze concurrent files or index entries, or detect changes fully restored between observations.
The basis is an observation token. It does not reserve an index lock or authorize a future write by itself.

## Applying a reviewed proposal

Application resolves the worktree-specific index and exclusively creates its `index.lock` before capturing the reviewed states again.
An existing lock causes refusal, including a symlink lock. Cleanup removes only a lock whose device and inode still match the opened file.
A nonregular or symlink index also causes refusal. Split indexes, sparse indexes and sparse checkouts are unsupported for writes.
These extra restrictions do not prevent otherwise supported previews.

The command copies the locked index into a private temporary directory beside it, preserving unrelated entries.
It writes captured raw blobs and feeds their exact modes and identities to Git's NUL-delimited `update-index --index-info` interface.
It verifies selected entries and compares unrelated staged inventories, including conflict stages and assume-unchanged/skip-worktree flags.
Tests also cover unrelated intent-to-add entries in a version-four index.
Selected entries with `unchanged` actions bypass updates, retaining their flags. Git may reset flags on selected entries that change.
The implementation uses Git's [alternate index](https://git-scm.com/docs/git#Documentation/git.txt-GITINDEXFILE) and [index-info plumbing](https://git-scm.com/docs/git-update-index#_using_index_info).

Before installation, the command rechecks filters, ignores, supported index configuration, selected working identities, complete live index bytes and lock ownership.
It syncs the prepared index bytes and renames the owned lock over the live index atomically.
Unrelated changes staged after preview survive because application copies the current locked index.
An observed index change during application causes refusal, including an unrelated change made by a writer that bypasses the lock.
An entirely unchanged proposal succeeds without replacing the index or writing objects.

`durability` describes the result:

| Field | Meaning |
|---|---|
| `index_replaced` | Whether the prepared index replaced the live index |
| `directory_synced` | True after directory sync; null when no replacement was needed |
| `warning` | Present if directory sync failed after installation |

A directory-sync failure reports `applied: true`, `directory_synced: false` and a warning because installation already happened.
Failures before the rename leave the live index untouched by `fr`; newly written unreachable blob objects may remain for Git to collect.
Normal failures remove temporary preparation files and the owned lock. Process termination can leave them behind, following Git's manual stale-lock recovery convention.
Do not remove a lock while another writer owns it.

The lock coordinates cooperating Git writers. Working files, configuration and repository directories are not locked.
Checks cannot detect changes restored between observations or prevent a writer that bypasses the lock from racing the final check and rename.
Keep repository configuration and directory topology stable during application.
This is atomic index replacement, without a durable staging journal or crash-recovery protocol for the whole operation.
`fr history undo` and redo concern source transactions; they do not reverse this index operation.
Review the resulting staging with `fr git diff PATH --staged` or `fr git changes --staged`.

## Configuration and assurance

The shared Git runner disables system/global configuration, filesystem-monitor and other Git hooks, optional locks and demand fetching of missing objects.
It requires Git support for `--no-lazy-fetch`. Repository configuration and includes remain available.
Content-filter checks cover every selected path. Unsupported filters cause refusal before source capture.
Ignore checks use repository ignore rules, with `core.excludesFile` fixed to `/dev/null`.
Configuration and attributes must remain stable during inspection.

The preview shares index inventory and working snapshot readers with explicit call context.
Projected modes reuse the anchored Git mode model, with assumptions documented in [Lean specifications](lean-specs.md).
Inventory interpretation, action classification, hash assumptions and snapshot consistency have regression evidence, without a general implementation proof.
Index locking, preparation, preservation and installation have regression tests, without a Lean correspondence proof or crash-consistency proof.
Tests cover no-write behavior, dirty repositories, basis drift, ignored files, conflicts, source/index races, linked worktrees and SHA-256 identities.
