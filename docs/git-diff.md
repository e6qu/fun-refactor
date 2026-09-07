# Git diff detail pages

```sh
fr git status --kind unstaged --limit 20
fr git diff src/main.rs --limit 20
fr git diff src/main.rs --limit 20 --cursor TOKEN
fr git diff src/main.rs --staged
fr git diff src/main.rs --since HEAD~1
```

Use [repository change pages](git-changes.md) to discover paths for the same comparison before requesting detail.
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
Use `--symbols` for changed declarations or `--calls` for snapshot-local call candidates, described below.
Cross-file relationships and transitive structural impact remain roadmap work.

Git inspection requires support for [`--no-lazy-fetch`](https://git-scm.com/docs/git); older Git versions refuse the command.

## Changed declarations

```sh
fr git diff src/main.rs --symbols --limit 20
fr git diff src/main.rs --symbols --staged
fr git diff src/main.rs --symbols --since HEAD~1
fr git diff src/main.rs --symbols --since HEAD~1 --cursor TOKEN
```

`--symbols` sets `view: "symbols"` and pages through declarations that overlap added or deleted lines, without source bodies or signatures.
Deleted lines select declarations from the before snapshot; added lines select declarations from the after snapshot.
Context lines do not select declarations. Insertions alone can therefore produce only after-side rows, even inside an existing function.
Newline markers share their preceding changed line and do not increase its count.
Mode-only, binary and empty-file changes can have metadata without declaration rows.

The view reads before-side blobs from Git's object database.
Staged after-side blobs also come from that database, independently of working files.
Other after-side snapshots come from regular working files, rejecting symlink traversal.
Each snapshot used for mapping must hash to its observed Git blob identity, without content conversion or object writes.
A changed file or conversion difference causes refusal before any page appears.
For example, a CRLF working file can differ from Git's normalized LF blob; ordinary diff inspection still works in that case.
Missing objects cannot trigger demand fetching. Parsing covers captured snapshots, without freezing subsequent repository changes.

Rows sort by side (before, then after), declaration start, descending end and extractor ID.
Each row contains `id`, `side`, `parent`, `kind`, `name`, `qualifier`, `exported`, byte `span`, `line`, `end_line` and `changed_lines`.
Line bounds are inclusive. A declaration ending at a line boundary excludes the following line.
`name` and non-null `qualifier` are excerpts with `text`, original `bytes` and `truncated`, capped at 256 UTF-8 bytes each.
Variables and parameters are outside this view. Remaining kinds follow the language extractor's declarations.

Hierarchy uses strict byte-span containment, as in project maps.
Every overlapping containing declaration also appears, so changed-line counts can repeat across ancestors and children.
These rows identify line overlap rather than semantic changes: multiple declarations sharing a changed line can all appear.
`parent` can refer to a row on an earlier page.
IDs are local to `structure.revision`, not project handles or identities shared across comparison sides.
There is no automatic rename or before/after declaration matching.

`structure.coverage` reports each side's status, observed `blob`, changed-line count and declaration count.
Parsed sides also include `language`, `gaps`, `mapped_lines` and `unmapped_lines`.
Mapped counts deduplicate lines across declarations. Unmapped lines can include imports, comments, top-level statements or extractor omissions.
A `parsed` status means extraction reports no known gaps; it does not establish complete language semantics.
Syntax or template gaps yield `partial` coverage with any available declarations.
Other statuses are `no-changed-lines`, `binary` and `unsupported-language`; mapped/unmapped counts remain null when mapping does not run.
Unsupported grammars, invalid spans, non-UTF-8 snapshots and inconsistent object identities cause refusal.

Language detection uses the extension only, without consulting the current filesystem for historical framework markers.
In particular, neighboring Helm chart files do not change a YAML snapshot's language.
The symbol view does not run dependency resolution or call analysis.
Use project navigation separately to inspect current declarations and candidate relationships.
The call view below inspects historical callers within the selected file. Cross-file historical relationships remain roadmap work.

Symbol cursors use a separate identity from line pages.
They bind the complete diff, tool version, full declaration result and coverage before paging.
Keep `--symbols`, the path and comparison unchanged when continuing. The limit can change.
Each side requires a complete snapshot and extraction before paging; pagination bounds response rows only.

The line-range predicate carries a source anchor, signature map and six Lean laws in `FrKernels.Git`.
Shared execution checks 1,728 boundary combinations, including machine-integer limits.
The proofs establish inclusive range membership, refusal outside or across reversed bounds, singleton behavior and preservation by enclosing ranges.
Git capture, blob-hash assumptions, parsers, declaration spans, hierarchy construction and report aggregation remain outside that proof.
See [Lean specifications](lean-specs.md) for proof assumptions.


## Calls touching changed declarations

```sh
fr git diff src/main.rs --calls --limit 20
fr git diff src/main.rs --calls --direction incoming
fr git diff src/main.rs --calls --staged
fr git diff src/main.rs --calls --since HEAD~1
```

`--calls` sets `view: "calls"` and returns call candidates touching the changed declarations selected above, without source bodies.
It cannot combine with `--symbols`. `--direction` requires `--calls` and accepts `incoming`, `outgoing` or `both` (default).
Before and after snapshots are analyzed independently, with the same blob checks, conversion refusals and extension-based language detection as symbol pages.
Only the selected file enters each index and hierarchy analysis. Source-dependent receiver inference reads that captured snapshot.
Neighboring working files do not supply historical targets. Imported or otherwise unresolved targets remain explicit unresolved rows.

Selection includes every declaration overlapping changed lines and the declarations and sites contained within those spans.
A changed class can therefore select calls inside unchanged sibling methods.
Incoming rows have a target declaration inside the selection; outgoing rows have a call site inside it.
`scope_relation` is `internal` when both conditions hold, including when filtering to one direction.
Otherwise it is `incoming` or `outgoing`. This describes containment, without claiming semantic change or runtime impact.
Sides with no changed lines do not run call analysis.

Each row contains `kind: "call"`, `side`, `scope_relation`, `caller`, `callee`, `site`, `confidence`, `origin`, `dispatch_candidate` and `status`.
Sites contain byte `offset` and one-based `line` and `column` coordinates.
Non-null endpoints contain `id`, `name`, `kind`, `qualifier`, declaration `line`, `changed_declaration` and `in_selection`.
`changed_declaration` reports direct line overlap; `in_selection` also includes declarations inside selected containers.
Endpoint IDs belong to that side's complete snapshot and are local to `structure.revision`.
They are not project handles, and endpoints need not appear in a changed-declaration page.
Names and qualifiers retain at most 256 UTF-8 bytes, with original lengths and truncation flags.

Resolved file-scope calls have a null caller and `caller_scope: "file"`.
Unresolved calls have a null callee and a bounded `name`; their caller can also be null.
Statuses distinguish `indexed-target`, `dispatch-candidate` and `unresolved`.
Confidence and origin retain the existing call graph's evidence, including weaker dispatch candidates.
An indexed target does not establish a unique runtime destination or permission to rewrite.

`structure.scope` is `calls-touching-changed-declarations`; `relationships` is `single-file-call-candidates`.
`structure.direction` records the normalized direction. Each analyzed side adds `calls` to its existing declaration coverage.
Call coverage reports `status` (`analyzed`, `partial` or `unsupported-language`), `scope: "single-file-snapshot"` and, when analysis runs, `cross_file: "not-collected"`.
Analyzed sides include hierarchy support and gaps, callable-node and edge counts, file-scope and unresolved-call counts, and `selected_rows`.
Graph counts cover the complete single-file snapshot; `selected_rows` counts only rows matching this selection and direction.
`analyzed` means no reported parser or hierarchy gaps; unresolved calls can still exist.
Binary, unsupported-extension and unchanged sides retain their declaration status without nested call coverage.

Rows sort deterministically within each side, with before rows first. No matching or edge subtraction occurs across sides.
Call cursors cannot continue line or symbol pages. They bind the complete diff, tool version, normalized direction, rows and coverage.
Continue with `--calls --cursor TOKEN`, retaining the path, comparison and direction. The limit may change.
Pagination bounds response rows; complete snapshot extraction and call analysis still run before paging.

The direction predicate has a source anchor, signature map and six Lean laws, proved without axioms.
Shared execution compares all 16 boolean inputs with Rust.
Those laws cover supplied selection flags; extraction, graph construction, enum mapping and complete report correspondence remain outside the proof.
Cross-file historical relationships and transitive impact remain pending.
