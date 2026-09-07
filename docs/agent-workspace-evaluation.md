# Repeated agent trials on the regex workspace

Four fresh agents completed two paired trials on the complete regex workspace at commit `2b527599eb9eea0dcc288c704584f242f26a5c61`.
It contains seven workspace packages, 227 Rust files and 5,553,380 Rust source bytes, without artificial background source.
All four pass independent acceptance. Across both repetitions, fr uses 25.6% more measured context than ordinary files.
The per-pair comparison varies with inspection and check-output choices; this evaluation does not establish a general context advantage.

## Paired results

| Repetition | Tool surface | Passed | Retrieved context tokens | Tool calls | Trial seconds |
|---|---|---|---:|---:|---:|
| 1 | fr | Yes | 11,967 | 28 | 196.6 |
| 1 | Ordinary files | Yes | 6,113 | 19 | 101.6 |
| 2 | fr | Yes | 11,603 | 27 | 162.1 |
| 2 | Ordinary files | Yes | 12,660 | 19 | 104.2 |

Mean retrieved context is 11,785 tokens for fr and 9,386.5 for ordinary files.
The fr arm uses 95.8% more context in repetition one and 8.3% less in repetition two.
Every trial has zero recorded refusals or tool failures and zero human task corrections.
No trials were interrupted, restarted or excluded. The earlier controlled rehearsal remains separate from these autonomous results.

Each agent discovered the existing regex-syntax helper and added a documented facade function in `src/lib.rs`.
Both declared checks pass in original, changed, undone and redone states, in the required order.
Undo and redo restore exact tracked bytes and modes while preserving an unrelated later edit.
Only the requested source file changes; both project and receiver indexes stay unchanged.
Each exported patch reproduces the final source in a clean receiver.
All four projects and all four receivers pass the independent 3,174-case append/allocation oracle.

The [retained manifest](../tests/agent-eval/results/2026-09-08-regex/manifest.json) identifies every prompt, transcript, score, patch and frozen skill file by checksum.
It records preparation commit `811591e`, runtime details and evaluator hashes.
The tested binary includes M4k implementation `6d0928b`; its SHA-256 is `6eccc6da5db83d295fa447089f6a0337e69a3adf5633df59c4fb60193026e856`.

## Where context went

| Visible output category | fr r1 | Files r1 | fr r2 | Files r2 |
|---|---:|---:|---:|---:|
| Skill reads | 2,825 | 0 | 2,825 | 0 |
| Project/source inspection | 2,310 | 1,442 | 2,603 | 2,755 |
| Declared checks | 3,051 | 3,328 | 3,051 | 8,463 |
| Changes and delivery | 2,897 | 467 | 2,240 | 566 |

The totals above exclude the prompt: 884 tokens per fr trial and 876 per file trial.
Generated requests add 1,306 and 1,214 tokens for fr, and 789 and 880 for files; those are excluded from retrieved context.
Both fr agents loaded the core skill and five relevant references, used exact-name lookup and avoided empty-result retries.
Both used `checks --quiet-success` and `--no-diff` for history writes after reviewing previews.
The first file agent also used quiet-success checks. The second used default check output, accounting for much of its higher total.
All agents had access to the same check command; output selection was an agent choice, not an assigned arm restriction.

Skill loading and transaction reports remain substantial fixed costs for this localized change.
Project inspection is bounded, but it is not consistently smaller than targeted ordinary source inspection in these trials.
The two fr trials spend 73.6 and 81.8 seconds inside instrumented tools, versus 4.9 and 2.7 seconds for files.
Repeated uncached project analysis dominates the fr tool time; bounded output does not imply inexpensive indexing.

The next optimization should examine repeated report metadata and skill-loading costs while preserving source bases, coverage, guards and reviewable edits.
A later task should require coordinated changes across files, since a larger repository alone does not test that workflow.

## Task and source

Add a documented public `regex::escape_into(pattern: &str, buf: &mut alloc::string::String)` function.
It must append escaped text, preserve existing buffer contents, avoid an intermediate escaped allocation and support builds without default features.
The task requires discovering and using functionality across the regex and regex-syntax package boundary.
Only the facade's `src/lib.rs` needs modification; this does not evaluate coordinated edits in several packages.
The prompt permits reuse of workspace functionality without identifying the helper's location.

The [source archive](../tests/agent-eval/regex/workspace.tar.gz) comes from the [pinned upstream repository](https://github.com/rust-lang/regex/tree/2b527599eb9eea0dcc288c704584f242f26a5c61).
GitHub supplied the [commit archive](https://codeload.github.com/rust-lang/regex/tar.gz/2b527599eb9eea0dcc288c704584f242f26a5c61).
Its SHA-256 is `7f8beace6ed6c94b2eec3c5aa92219e10e73bd8ad696035877bed01d3ee47255`.
The existing cached regex release's Cargo metadata identifies the same upstream commit.
The archive includes upstream's MIT and Apache-2.0 notices; separate copies accompany the fixture.

Source text and manifests remain unchanged at preparation time.
Extraction rejects links and unsafe member paths, and normalizes regular/executable modes to 0644/0755, as in a Git checkout.
The archive's original 0664/0775 modes otherwise differ from files recreated by Git patch application.
This normalization happens before either arm's initial snapshot.

The fixture adds a separately [pinned Cargo.lock](../tests/agent-eval/regex/Cargo.lock), declared checks and disposable Git metadata.
The lock SHA-256 is `7ae225f5fbac82509d5c259dd3613a809c75a8bd4e2124b9cb69ed29d98de3a0`.
Preparation tracks the lock even though upstream ignores it. Checks use `--locked --offline`; manifests retain their actual workspace path dependencies.
The initial dependency fetch resolves the lock once. Later runs use those versions and need no network after fetching them.

## Checks and independent oracle

Every validation stage runs both declared checks together:

- `upstream`: the regex and regex-syntax library test suites, totaling 154 tests in the initial preflight. This excludes integration and documentation tests.
- `minimal`: compile regex with no default features, preserving the public facade's allocation-only build support.

The caller-side oracle uses an explicit metacharacter reference and 3,174 input/prefix combinations, with two appends per combination.
Cases include every metacharacter, ordinary punctuation, empty inputs, whitespace, NUL, Unicode and repeated longer text.
An allocation-counting harness requires no allocation when the destination already has sufficient capacity.
The original workspace fails because the requested facade API is absent; a dependency or unrelated compiler error cannot establish that baseline.
The oracle runs against the changed project and its independent patch receiver.
These finite checks do not prove correctness for every possible string or establish general allocation behavior.

The [controlled rehearsal](../tests/agent-eval/regex/rehearsal.json) uses a prescribed implementation and the real fr binary.
It passes saved-plan application, original/changed/undone/redone checks, exact snapshots, unrelated-edit preservation and patch delivery with unchanged indexes.
Negative controls compile but fail the oracle when they clear the prefix, skip escaping or allocate an intermediate string.
The report identifies the frozen binary, source archive and dependency lock by digest.
This rehearsal supplies infrastructure and behavioral evidence; it is not an autonomous trial or a context measurement.

The rehearsal exposed a product prerequisite: regex denies missing documentation on public APIs.
M4k allows leading `///` and `/** ... */` documentation during Rust function insertion, with the existing size, syntax and history guards.
The signature report excludes documentation, and a separate field identifies its source region.
Other outer attributes and ordinary surrounding comments still refuse. See [function authoring](body-authoring.md#declaration-insertion).

## Reproduction and trial design

Four fresh collaboration agents ran in two pairs: repetition one completed before repetition two started.
Each agent began without conversation history, read only its assigned generated prompt and used the instrumented tools.
The same inherited model and effort applied to all four, with no overrides or pinned serving-model build available.
No source, solution or first-pair results were supplied to the second pair.
The frozen binary, prompts, skills, harness and oracle stayed unchanged throughout the trials.
The boundary is cooperative, rather than an operating-system sandbox.

Both agents in each pair shared one host and used preflight-warmed project builds and the same dependency cache.
Repository inspection, documentation work and scoring of completed trials also overlapped active agents.
No full repository test gate ran during either pair.
The harness disables fr's cache for every invocation; timings therefore include repeated project indexing.
Trial seconds run from the first instrumented event to the last, excluding session preparation and later scoring.
These timings cannot establish production latency or a general speed comparison.

Retrieved context counts the generated prompt and each visible instrumented payload once, including loaded skill references.
Generated request tokens are reported separately. System context, hidden reasoning, commentary and transport framing are excluded.
The count omits repeated prefix processing, caching and billed usage.
Scoring uses tiktoken 0.12.0 and the checksum-pinned o200k_base vocabulary from the [initial protocol](agent-acceptance.md).
The facade reserializes CLI JSON; these counts are not directly comparable to the [controlled history stdout measurement](agent-context-followup.md#subsequent-history-completion-reports).
Two repetitions on one task and one host cannot establish general context efficiency or a reliable success rate.

Bootstrap dependencies from a disposable copy outside any Cargo workspace:

```sh
python3 tools/regex-workspace-check.py unpack /tmp/fr-regex-deps
CARGO_HOME="$PWD/target/cargo-home" cargo fetch --manifest-path /tmp/fr-regex-deps/Cargo.toml --locked
python3 tools/regex-workspace-check.py check --fr target/debug/fr
```

The controlled check uses its own temporary projects. It does not change the dependency-bootstrap source or run agents.
It needs Cargo, rustc and Git. Normal CI runs the archive, protocol and scoring regressions without building this additional workspace.
The workspace rehearsal is an explicit check after its locked dependency cache is ready.

Freeze the intended binary before preparing four sessions:

```sh
python3 tools/agent-eval.py prepare --project regex --repetitions 2 --out /tmp/fr-regex-agents --fr /path/to/frozen/fr
```

This produces `regex-escape-into-fr-r1`, `regex-escape-into-files-r1`, `regex-escape-into-fr-r2` and `regex-escape-into-files-r2`.
Each session has its own project, clean receiver, skill snapshot, original oracle result and generated prompt.
Use a fresh agent for each prompt, with the same model and effort and no source or solution sharing between sessions.
Report execution order, host contention, refusals and any human intervention. Two repetitions remain a small sample.
Agents must run all declared checks together at each stage; grading refuses a stage that omits either check.
The read/search/edit boundary and token accounting follow the [initial protocol](agent-acceptance.md).

Score every completed session with the pinned tokenizer before recording the complete cohort:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py score /tmp/fr-regex-agents/regex-escape-into-fr-r1
python3 tools/agent-eval.py record /tmp/fr-regex-agents /tmp/fr-regex-evidence --execution-note 'Describe the actual runtime, isolation and interventions here.'
```

Repeat scoring for the other three sessions. Recording checks the planned cohort and preserves scored failures even when no patch exists.
Runtime provenance comes from the supplied note; the recorder does not assume that fresh agents ran.
Behavioral replay refuses failed trials. Token auditing reads each retained prompt and instrumented transcript.
Default preparation still creates the original four strsim sessions, and both earlier evidence bundles remain immutable.

The retained cohort passes exact token auditing and behavioral replay of all four patches:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-08-regex
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-08-regex
```

An opt-in integration regression runs the same workspace replay after the dependency bootstrap above:

```sh
CARGO_HOME="$PWD/target/cargo-home" CARGO_NET_OFFLINE=true cargo test --test agent_acceptance recorded_workspace_patches_pass_checks_oracles_and_exact_reversal -- --ignored
```

Default acceptance tests retain the sixteen harness regressions and eight earlier patch replays without requiring the additional workspace dependencies.
The M4k full native/WASM gate and strict kernel verification passed before these trials.
This milestone adds evidence, replay coverage and documentation; production code and formal claims are unchanged.

## Subsequent check-report reduction

M4m adds opt-in `checks --no-declarations` for executions after reviewing the configuration listing.
It retains the basis, result names, outcomes, diagnostics, omitted-byte counts and names of unselected checks.
The [controlled comparison](project-checks.md#controlled-report-measurement) projects 33.7% less check-execution output in each retained fr trial.
It also validates real CLI reports against the pinned workspace. This is not a fresh autonomous measurement, and the scores above remain unchanged.
