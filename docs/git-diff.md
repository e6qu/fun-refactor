# Git diff detail pages

```sh
fr git status --kind unstaged --limit 20
fr git diff src/main.rs --limit 20
fr git diff src/main.rs --limit 20 --cursor TOKEN
fr git diff src/main.rs --staged
fr git diff src/main.rs --since HEAD~1
```

The native command requires Git and prints JSON in both output modes.
It inspects one literal UTF-8 file path relative to the repository root, even when `-C` selects a nested directory.
Paths containing whitespace, glob characters or Git pathspec magic retain their literal meaning.
Use `--` before a path beginning with a dash. Linked worktrees select their own files and index.
Inspection does not load transaction history or write source, history or index data.

## Comparison and metadata

| Selection | Before | After | `scope` |
|---|---|---|---|
| Default | Index | Working tree | `index-to-worktree` |
| `--staged` | HEAD, or empty tree on an unborn branch | Index | `head-to-index` |
| `--since REV` | Resolved commit | Working tree | `commit-to-worktree` |

`--staged` and `--since` cannot combine. `--since` uses one commit directly, without computing a merge base.
These comparisons follow [Git diff semantics](https://git-scm.com/docs/git-diff), including tracked membership, attributes and built-in content conversion.
They do not establish raw filesystem snapshot equality or provide an edit precondition.

`schema` is 1. Metadata includes `repository_root`, `path`, `scope`, `base_commit`, `diff_revision`, `changed` and `binary`.
`base_commit` is null for the default comparison and for staged inspection on an unborn branch.
`change` is null when unchanged. Otherwise it contains `status` (`A`, `D` or `M`), before/after modes and before/after object IDs.
Modes are Git octal strings (`100644` or `100755`), with null for an absent side.
Object IDs come from the [raw diff record](https://git-scm.com/docs/diff-format); all-zero IDs become null.
In particular, a working-tree result often has a null `after_oid` even when Git computes a blob identity elsewhere in the patch.

`renames: "disabled"` means each deletion and addition requires its own path query.
`submodules: "unsupported"` declares the regular-file scope.
Unmerged entries, symlinks, submodules, directory selections, parent traversal and paths absent from both selected bases cause refusal.
Default inspection cannot select a staged deletion absent from the index; use `--staged` or `--since`.
Untracked files require another inspection interface. Non-UTF-8 patch text and malformed records also cause refusal.

## Rows and limits

`counts` reports total `hunks`, `added` lines and `deleted` lines before pagination.
Git's binary classification produces metadata without source rows, with null added/deleted counts.
Mode-only changes and empty-file additions/deletions can also have zero rows while `changed` remains true.

Each entry has one of these `kind` values:

- `hunk`: zero-based `number`, `old` and `new` ranges containing `start` and `count`, and a `heading` excerpt.
- `line`: `hunk`, `change` (`context`, `add` or `delete`), nullable `old_line`/`new_line`, and a `content` excerpt.
- `no-newline`: `hunk` and the preceding line's coordinates, identifying sides without a terminal newline.

Line coordinates start at one; empty hunk ranges retain Git's boundary coordinates, which can be zero.
Each excerpt contains `text`, original byte length `bytes`, and `truncated`.
Source excerpts retain at most 1,024 bytes; headings retain at most 256 bytes, stopping at UTF-8 boundaries.
Source excerpts omit the patch indicator and newline delimiter but preserve carriage returns.
Hunks request three context lines. A page can begin or end inside a hunk.

`--limit` defaults to 50 and accepts 1 through 500 rows, including headers and newline markers.
`page` contains `total`, `returned`, `before`, `remaining` and `next`.
Continue with `page.next` as `--cursor`, retaining the same path and comparison. The limit may change.
Cursors bind the canonical repository root, path, comparison, resolved commit and complete observed diff bytes before excerpt clipping.
A changed observation or query causes refusal; restart the query.
This detects changes beyond visible excerpt limits, but cannot detect filesystem changes that Git omits from its diff.

Git still collects the complete selected diff, and the parser retains all rows before paging.
The row limit bounds response rows, not collection work, file size or total memory.
Inspection does not freeze concurrent files, index, attributes or configuration across Git subprocesses.
Use transaction basis checks before applying edits.

## Configuration and assurance

`configuration: "repository-only-without-content-filters"` identifies the selected settings.
The shared runner clears inherited `GIT_*` overrides, disables system/global configuration and external attributes, and retains repository configuration and its includes.
It disables filesystem-monitor hooks, optional locks and demand fetching of missing objects.
Diff inspection also disables external diff commands, text conversion, automatic index refresh and rename detection.
It fixes the diff algorithm to Myers with three context lines and disables the indent heuristic.
Before collecting a diff, it rejects content filters on the selected literal path.
Filters on unrelated paths do not prevent inspection; configured drivers named `unset` or `unspecified` remain unsupported.
Configuration and attributes must remain stable during collection.

`diagnostics` retains at most 16 KiB of Git stderr; `diagnostics_truncated` indicates omitted bytes.
Successful inspection exits zero even when changes exist. Setup, unsupported input and parsing errors return failure without partial pages.

Page sizing reuses the Lean-anchored pagination helper.
Parser and CLI tests cover coordinates, excerpts, cursors, comparison bases, unusual paths, conflicts, linked worktrees and guarded Git execution.
They do not prove parser or Git execution correspondence with Lean.
Symbol hierarchy, callers and structural impact since a revision remain roadmap work.

Git inspection requires support for [`--no-lazy-fetch`](https://git-scm.com/docs/git); older Git versions refuse the command.
