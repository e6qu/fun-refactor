# Git staging previews

```sh
fr git stage src/main.rs src/api.rs
fr git stage src/main.rs src/api.rs --basis TOKEN
```

`fr git stage` currently previews proposed index entries and prints JSON in both output modes.
It reports `operation: "stage-preview"`, `applied: false` and `write_supported: false`.
It does not stage files, write Git objects, create a transaction or accept `--write`.
Installing a prepared index is the next milestone.

## Selection and proposed entries

Provide one through 32 literal repository-relative file arguments. Paths normalize, deduplicate and sort before reporting.
Argument order and duplicates do not affect the basis. Use `--` before a path beginning with a dash.
Nested `-C` directories still select repository-relative paths. Linked worktrees select their own index and working files.
Directories, absolute paths, parent traversal, symlink traversal, selected index symlinks and submodules cause refusal.
A selected unmerged index entry also causes refusal. Unrelated conflicts do not block the preview.

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
All validation completes before any result appears. A failure emits an error without partial entries.
The checks do not freeze concurrent files or index entries, or detect changes fully restored between observations.
The basis is an observation token. It does not reserve an index lock or authorize a future write by itself.

## Configuration and assurance

The shared Git runner disables system/global configuration, filesystem-monitor hooks, optional locks and demand fetching of missing objects.
It requires Git support for `--no-lazy-fetch`. Repository configuration and includes remain available.
Content-filter checks cover every selected path. Unsupported filters cause refusal before source capture.
Ignore checks use repository ignore rules, with `core.excludesFile` fixed to `/dev/null`.
Configuration and attributes must remain stable during inspection.

The preview shares index inventory and working snapshot readers with explicit call context.
Projected modes reuse the anchored Git mode model, with assumptions documented in [Lean specifications](lean-specs.md).
Inventory interpretation, action classification, hash assumptions and snapshot consistency have regression evidence, without a general implementation proof.
Tests cover no-write behavior, dirty repositories, basis drift, ignored files, conflicts, source/index races, linked worktrees and SHA-256 identities.
