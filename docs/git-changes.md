# Repository change pages

```sh
fr git changes --limit 20
fr git changes --staged
fr git changes --since HEAD~1 --limit 20
fr git changes --since COMMIT_ID --cursor TOKEN
```

The native command requires Git and prints JSON in both output modes, without source or patch bodies.
It selects the entire repository containing `-C`; linked worktrees select their own working files and index.
It does not load transaction history or write source, history or index data.

## Comparison and selection

The default compares the index with working files. `--staged` compares HEAD with the index, including unborn branches.
`--since REV` resolves one commit and compares it directly with working files, without computing a merge base.
`--staged` and `--since` cannot combine.
These selections follow [Git diff semantics](https://git-scm.com/docs/git-diff), including tracked membership and built-in content conversion.
Untracked files are excluded. Use [Git status](git-status.md) to discover them.

Use a returned path with [Git diff details](git-diff.md):

```sh
fr git changes --since HEAD~1
fr git diff src/main.rs --symbols --since COMMIT_ID
```

Use `--calls` instead of `--symbols` for call candidates touching changed declarations within each file snapshot.
Copy `base_commit` from the first report to pin the second command's commit basis.
Pass paths literally, including whitespace or pathspec-like characters. Put a path beginning with a dash after `--`.
Discovery and detail commands observe independently; a path or its state can change between them.
The default and staged comparisons also observe the current index independently.

Rename detection is disabled, so renames appear as deletion/addition paths where both belong to the selected comparison.
Submodules, including transitions involving gitlink modes, are outside this report's scope.
Any unmerged index path causes refusal; inspect conflicts with `fr git status`.
Symlinks and ordinary type changes remain visible, with a gap explaining why regular-file detail inspection cannot select them.

## Fields and counts

`schema` is 1. Metadata includes `repository_root`, `scope`, `base_commit`, `changes_revision`, `identity` and `clean`.
Scope is `index-to-worktree`, `head-to-index` or `commit-to-worktree`.
The commit identity is null for the default comparison and staged inspection on an unborn branch.
`clean` means the selected scope contains no changed paths; it does not account for untracked files or submodules.

Each entry contains:

- `path`: a repository-relative UTF-8 path, preserving whitespace through JSON escaping.
- `status`: `A`, `D`, `M` or `T` for addition, deletion, content/mode modification or type change.
- `before_mode` and `after_mode`: Git octal strings, or null for an absent side.
- `before_oid` and `after_oid`: raw Git object identities, converting all-zero IDs to null.
- `binary`, `added_lines` and `deleted_lines`: binary classification and nullable numeric line counts.
- `detail_candidate` and `detail_gap`: whether the observed modes and state fit regular-file diff inspection, or `non-regular-file` as the gap.

Working-tree after IDs commonly remain null in [raw diff records](https://git-scm.com/docs/diff-format).
Numeric statistics identify binary files with null line counts. Mode-only and empty-file changes can have zero counts while remaining changed paths.
A detail candidate still requires fresh selection and validation by `fr git diff`; it does not promise a supported language or complete symbol extraction.

`counts` reports whole-comparison path totals: `paths`, `added`, `deleted`, `modified`, `type_changed`, `binary` and `detail_candidates`.
The four status counts partition `paths`. Binary and detail-candidate counts overlap those categories.
They count paths; individual entry fields carry line counts.
Paths sort lexically before pagination.

## Continuation and identity

`--limit` defaults to 50 and accepts 1 through 500 paths.
`page` contains `total`, `returned`, `before`, `remaining` and `next`.
Pass `next` unchanged as `--cursor`, keeping the same comparison. The limit may change.
Cursors bind the canonical repository root, comparison, resolved commit and complete sorted metadata result before paging.
A changed observation or query causes refusal; restart the query.

`identity: "change-metadata"` states the observation limit.
Further working-file edits can retain the same modes, status, raw IDs and line counts, leaving the cursor valid.
For example, replacing one changed line with different text of the same shape may preserve all reported metadata.
Use diff, symbol or call pages for content-sensitive inspection, and transaction basis checks before edits.
None of these Git queries freezes concurrent repository state.

## Configuration and assurance

The shared runner requires Git support for [`--no-lazy-fetch`](https://git-scm.com/docs/git), preventing demand fetching of missing objects.
It clears inherited `GIT_*` settings, disables system/global configuration and external attributes, and retains repository configuration and its includes.
It also disables filesystem-monitor hooks and optional locks.
Change collection disables external diff commands, text conversion, automatic index refresh and rename detection.
It fixes the diff algorithm to Myers and disables the indent heuristic.

Before collecting changes, the command checks content-filter attributes on the union of index and selected commit paths.
A content filter anywhere in that set causes refusal, including clean paths outside the returned page.
Configured filter drivers named `unset` or `unspecified` also cause refusal.
Configuration and attributes must remain stable during collection; preflight checks do not isolate concurrent changes.

The parser joins NUL-delimited raw records and numeric statistics from one Git invocation.
It rejects malformed records, duplicates, inconsistent states, unsafe paths, unsupported path encodings and overflowing line counts.
A raw modification with equal modes, a known before ID, no after ID and no statistics represents a stat-only candidate and does not appear.
Tests cover this case without allowing automatic index refresh.

`diagnostics` retains at most 16 KiB of Git stderr; `diagnostics_truncated` indicates omitted bytes.
Successful inspection exits zero even when changes exist. Failures return no partial page.
The page size bounds output paths, not path length, Git collection work, source sizes or total internal memory.
The report collects metadata for the complete comparison before paging, without collecting patch bodies.

Page sizing reuses the Lean-anchored pagination helper.
Parser and CLI regressions cover state/count joins, malformed data, cursors, scopes, filters, conflicts, worktrees and SHA-256 detail handoff.
Git execution, metadata correspondence and aggregation remain outside the formal model.
