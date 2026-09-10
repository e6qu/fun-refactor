# Handing fr to an agent

The portable [fr skill](../skills/fr/SKILL.md) teaches bounded project exploration and reviewed code changes.
Its references separate exploration, authoring, built-in changes, recipes, checks, history, recovery, patches, Git administration and Lean.
The entrypoint routes to those references only when the task needs them.

## Distribution and use

The native release packaging includes `skills/fr/` beside the binary.
From a source checkout, use that same directory.
Copy the whole `fr` skill folder into the receiving agent's skill location, or point the agent at its `SKILL.md` directly.
Keep the `references/` directory beside the entrypoint; all skill links stay inside the portable folder.
Packaging does not install the skill globally or alter the agent's configuration.

Make the matching `fr` binary available to the agent. Ordinary exploration and source history work without Git or Lean.
Git commands need Git; strict model verification also needs the project's Lean/Lake toolchain.
Use the target project's root and existing task authorization.
A request to inspect or export does not grant permission to commit or publish.

Known names can go directly to `project find`; the initial map remains available when the hierarchy needs inspection.
The default lookup and introductory map each request at most twelve rows.
Subsequent queries select a declaration, read its signature and relationships, and request source slices only as needed.
`project find --source --bytes N` can return needed implementations during lookup, sharing its source-text budget across the page.
Body authoring accepts project handles; built-in refactorings use names or source positions.
The authoring reference covers TypeScript function bindings inside parentheses and type-only assertions, with local lookup and explicit refusal boundaries.
It uses saved transaction IDs for exact plan application, and keeps source history separate from Git bases and journals.
History and check commands do not require refreshing project handles after every write.
For a targeted edit, the entrypoint starts with Author; Explore covers pagination, relationships and broader discovery when needed.
The authoring reference avoids loading recipe vocabulary for a body edit; patch guidance loads Git administration only when needed.
History links to a separate recovery reference when a pending operation or interrupted lock needs inspection.
An explicitly file-scoped lookup already returns its file handle in `root`; insertion can reuse it while the source revision remains current.
Unscoped and directory-scoped roots cannot substitute for file handles. A file map remains available when the needed handle is missing.
The check example uses `--quiet-success` and retains diagnostics for failed commands.
History writes use `--no-diff` after reviewing the saved plan or transition preview, retaining completion metadata without repeated diff text.
Project and author calls can reuse a reviewed `context_basis`. A complete saved author diff provides a separate transaction basis for compact forward apply and redo reports.
The introductory lookup guidance distinguishes a full name from a fragment, which needs `--contains`.

## Executable evidence

Run the examples against a chosen build, including a downloaded release binary:

```sh
python3 tools/check-agent-skill.py --fr /path/to/fr
```

The checker uses temporary projects and executes every fenced shell example from the skill files.
It obtains real handles, positions and transaction IDs from command output, then substitutes them into the example commands.
The source fixture checks:

- Bounded declaration lookup, selected signature, relationships, source slice, call edges, candidate tests and coverage gaps.
- A recipe preview with file-count expectations, then a saved two-file Python rename.
- Refusal of a stale plan and an old project handle after a source change.
- Applied behavior, undo/redo and preservation of an unrelated later edit.
- Exported patch application in a separate Git receiver, with both indexes unchanged.

A Rust fixture executes the authoring example using source and the root from one file-scoped lookup, without separate source or file-map queries.
It rejects an unscoped root, inserts a documented wrapper and compiles it with `deny(missing_docs)`.
A compiled caller checks the wrapper's result; stale-handle refusal, exact undo/redo and unrelated-edit/index preservation also pass.

The Lean fixture checks a stale source anchor, reviewed hash synchronization and a real Lake build.
It then changes the theorem to a false proposition and requires verification to fail with one JSON report.
Separate JSON regressions cover missing declarations and invalid signature maps.
The checker does not exercise interrupted-write recovery or every optional Git operation in the prose; those retain their dedicated CLI regressions.

The standard native test gate runs this checker through `tests/agent_skill.rs`.
The skill validator also checks its frontmatter and unfinished placeholders during authoring.
The checker enforces a 1.5 KiB entrypoint budget, a 4 KiB budget per reference, a 7 KiB budget for each declared task route and valid links inside the portable folder.

The initial macOS run against the development binary executed 31 fenced command examples.
Declared project-check listing and execution brought the checker to 33 examples; M4n's targeted authoring workflow raised that to 37.
M4o combines its lookup and source read. Later project selection, plan compaction and Lean adoption examples bring the current total to 42.
The execution example combines quiet-success output with declaration omission after reviewing the configuration basis.
The initial measurements were:

| Measure | UTF-8 bytes |
|---|---:|
| Skill entrypoint | 2,120 |
| References exercised by the initial fixture | 11,680 |
| Exploration command output | 11,223 |
| Fixture Python source | 66,735 |

These initial figures describe a synthetic project with deliberately unrelated background source.
They are not model token counts or evidence of autonomous agent success.
Path lengths and later documentation changes can change the byte counts; the checker prints fresh measurements. The PR 1 entrypoint is currently 1,611 bytes.
The native packaging change has a local archive check; release uploads and other platform builds require their normal release jobs.

The [first real-agent evaluation](agent-acceptance.md) now records two fr tasks and their ordinary-file comparisons on a pinned public Rust release.
Both fr tasks pass independent behavioral oracles, declared checks and reversible patch workflows without human corrections.
The [follow-up evaluation](agent-context-followup.md) reduces fr context by 34.0% and 24.5% against its first trials.
The fresh file trials also improve and still use less context. The evidence supports these workflows while leaving broader efficiency open.
The same report includes a subsequent controlled comparison of smaller history completion reports; this adds no new autonomous-agent result.

## Targeted reading measurement

The [retained M4n comparison](../tests/agent-eval/skill-context.json) measures that skill revision against both frozen regex fr trials.
It uses the same numbered-line read payloads and pinned tiktoken 0.12.0/o200k_base tokenizer as the agent harness.
All required targeted-edit references are counted: the entrypoint, authoring, checks, history and patch guidance.

| Reading path | References including entrypoint | Tokens per trial |
|---|---:|---:|
| Recorded regex skill reads | 6 | 2,825 |
| M4n skill, loading those same six files | 6 | 2,960 |
| M4n targeted-edit route | 5 | 2,375 |

The targeted route is 450 tokens, or 15.9%, below recorded skill reads and 19.8% below M4n's six-file route.
The comparison includes M4m's check guidance and the new authoring example, so it does not isolate a single wording change.
Unselective loading would increase context by 4.8% in this controlled comparison.
Both fr agents in the later [coordinated cohort](agent-coordinated-evaluation.md) follow the five-file route, reading 2,851 tokens of the newer skill.
Those trials do not isolate the routing change or reproduce the earlier skill revision's reading cost.
An interrupted write requires the separate recovery reference, adding 203 tokens under this counting method.
Pagination, relationship queries and broader discovery still require the relevant exploration guidance.
These are conditional reading costs, not autonomous task results or total-context savings.

The first regex fr trial also queried a file map solely for a handle already returned by its file-scoped lookup.
The audit confirms an identical root/revision and no intervening source edits; that map payload accounts for 387 tokens.
The second trial lacked an earlier lookup scoped to the same file, so its map remains necessary under this rule.
The executable Rust example validates reuse and stale-root refusal; the trace audit does not estimate a general latency improvement.

```sh
python3 tools/skill-context.py
target/agent-eval-venv/bin/python tools/skill-context.py --tokens
```

The report retains M4n's read payloads, skill digests and the frozen input-manifest digest. Original transcripts and scores remain unchanged.
Rerunning the script after skill updates prints new counts; the retained report identifies its measured revision's files.

PR 7 keeps the same five-file targeted route while removing repeated explanations from its
entrypoint, authoring, checks, history and patch references. Raw Markdown falls from 9,937 to
6,074 bytes for that route, a 38.9% reduction. The complete bundle falls from 24,691 to 20,828
bytes. Under the pinned tokenizer and numbered-read framing used by `skill-context.py`, the
same route falls from 2,699 to 1,693 tokens, a 37.3% reduction. It is 269 tokens below the
1,962-token route in the frozen v2 projection. These measurements make no agent-success claim.
The executable checker covers the route and enforces bounded routes for built-in changes,
recipes, recovery, exploration, Lean and Git administration as well.

M4o subsequently combines lookup and source inspection under one page budget.
The [controlled source-lookup comparison](project-context-evaluation.md#bounded-source-during-name-lookup) measures that command composition separately from these skill-reading costs.

## Remaining roadmap

M4a provides the introductory handoff and executable command examples.
Body replacement supports Rust, Go, Java, TypeScript and TSX declarations and methods, plus TypeScript/TSX function bindings.
Go supports named functions and receiver methods, including generic headers; interface specifications and variables containing function literals refuse.
Java supports methods, constructors and default interface methods with bodies. Abstract and bodyless interface methods refuse.
Supported TypeScript/TSX bindings can wrap the function in parentheses and type-only assertions. Arrows accept expression or block bodies and can move between them.
Calls and conditionals around the initializer still refuse.
Rust function declaration replacement can change signatures and implementations together, preserving the name and outer attributes.
Declaration insertion adds a Rust function through a file, inline module, exact impl method or trait handle, retaining all existing source bytes.
Module and trait insertion use the container row's handle. An existing direct method handle selects its exact enclosing impl or trait body.
The fragment stays verbatim; bodyless functions are trait-only, while free functions, nested functions, external modules and empty impls remain unsupported targets.
It accepts leading outer documentation comments, so agents can satisfy a project's missing-docs lint without changing crate policy.
Authoring batches combine up to 32 disjoint operations from one revision, with shared coverage and one history transaction.
Declared project-check selection now has a configuration digest and bounded execution reports.
Broader real-agent evaluation, context optimization and further authoring operations remain open.
M5 still owns automated Lean package initialization and model scaffolding.
The skill does not claim complete framework migration, worktree undo/redo or general implementation verification.
