# Git status pages

```sh
fr git status --limit 20
fr git status --kind unstaged --limit 20
fr git status --kind unstaged --cursor TOKEN
```

The native command requires Git and prints JSON in both output modes, without source or patch bodies.
It inspects the entire repository containing `-C`, even when that directory is nested.
Linked worktrees use their own index and working files.
Status inspection does not load transaction history, so an invalid journal does not prevent this command.

## Rows and counts

`--kind` selects `all`, `staged`, `unstaged`, `untracked` or `conflicted`; the default is `all`.
Rows sort by repository-relative UTF-8 path. Unsupported encodings and malformed Git records cause refusal.
The parser follows Git's [porcelain v2 format](https://git-scm.com/docs/git-status).
Each row includes `path`, raw `xy`, `index_status`, `worktree_status`, `untracked`, `conflicted` and `submodule`.
Named states include modified, added, deleted, type-changed, renamed and copied; an unchanged side is null.
Rename/copy rows also include `original_path` and `similarity`.
Rename detection uses a 50% similarity threshold and remains a Git heuristic.
Paths retain whitespace and Unicode through JSON escaping.

Conflict rows keep both named side statuses null: their raw XY describes conflict sides, not ordinary staged/unstaged changes.
Untracked rows have `xy: "??"` and null side statuses.
The `counts` object covers all observed paths, regardless of filter or page.
Its fields are `paths`, `staged`, `unstaged`, `untracked` and `conflicted`.
A mixed file contributes to both staged and unstaged counts; conflicts contribute only to the conflict count.
Therefore the category counts need not sum to `paths`.

Untracked directories expand to files. Repository ignore rules still apply; ignored files do not appear.
Submodules are excluded through Git's `--ignore-submodules=all`, including staged gitlink changes.
The report declares `submodules: "ignored"`. Its `clean` value means no changes in this selected scope.

## Continuation and identity

`--limit` defaults to 50 and accepts 1 through 500.
The `page` object reports `total` for the selected kind, `returned`, `before`, `remaining` and `next`.
Pass a non-null `next` token unchanged as `--cursor`; keep the same kind but choose a different limit if needed.
Cursors bind the canonical repository root, branch/HEAD observation, sorted raw entry fingerprints and kind.
A changed observation or query fails with a stale-cursor error; restart the query.

`status_revision` hashes those observations. It is not a source revision or an edit precondition.
Further edits to an already modified or untracked file can leave Git's status records unchanged.
The command does not freeze the repository or provide an atomic snapshot across Git subprocesses.
Use transaction basis checks for edits and recorded patch checks for receiving-file compatibility.

Other metadata includes `schema: 1`, `repository_root`, `scope: "repository"`, `kind`, `branch`, `head` and `unborn`.
`branch` preserves Git's reported label, including `(detached)`; `head` is null for an unborn repository.
`diagnostics` contains at most 16 KiB of Git stderr, with `diagnostics_truncated` indicating omitted bytes.
Successful inspection exits zero even when changes or conflicts exist. Setup or parsing failures use the usual CLI errors.

## Configuration and limits

`configuration: "repository-only-without-content-filters"` identifies the selected settings.
Git reads repository configuration and its includes, plus repository attributes and ignore files.
The runner clears inherited `GIT_*` overrides and disables system/global configuration, external attributes and external ignore files.
It disables filesystem-monitor hooks and optional index locks, preserving index bytes during inspection.
Results can differ from a Git command using user or system settings.

Before collecting status, the command checks filter attributes for every tracked path, deduplicating unmerged entries.
Any tracked content filter causes refusal, including clean paths outside the requested page or kind.
Configured filter drivers named `unset` or `unspecified` also cause refusal because they overlap attribute-report states.
Configuration and attributes must remain stable during collection; these checks do not isolate concurrent changes.

Output row counts are bounded. Git still enumerates the repository and the command collects the complete observation in memory.
Collection work, individual path lengths and total internal memory are not bounded by the page limit.
There are no change hunks, ahead/behind counts, structural impact, staging or commit operations in this command.

Page sizing reuses the existing Lean-anchored pagination helper.
Parser and Git regression tests cover status classes, malformed records, cursors, conflicts, linked worktrees and preservation of index bytes.
These checks do not prove correspondence between Git execution, the parser and a Lean status model.
