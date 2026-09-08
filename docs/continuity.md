# Development continuity

M4o is complete: bounded source in `project find` combines lookup and needed implementation detail.
The user authorized local commits. Publishing and pushing remain outside this request.
M4l's four passing workspace trials are retained in `3c8231c`; preparation is `811591e` and documented insertion is `6d0928b`.
M4m's smaller check reports are committed in `58b53bd`; M4n's targeted skill guidance is `1c44863`.

## Current implementation

`project find NAME --source --bytes N` appends source slices under one raw UTF-8 byte budget shared across the returned page.
The range is 4 through 65,536 bytes, defaulting to 2,048. Slice metadata and handles remain when later rows receive no source bytes.
Resume a non-null `source.next_offset` with `project show HANDLE --source --offset NEXT --bytes N`.
Row cursors bind source mode and budget; ordinary lookups retain their previous reports and cursor identities.
Both find and show call the same source-slice helper, which reuses the existing modeled page-length arithmetic.
Use show separately when positions, child counts or relationships are needed.

The controlled report is `tests/agent-eval/find-source-context.json`; reproduce it with `tools/find-source-context.py --fr PATH --tokens`.
Two prescribed regex queries preserve lookup metadata and complete selected source while using about 40% fewer visible payload tokens.
The comparison checks unchanged tracked source and index bytes and records actual arguments, payloads and binary/source digests.
It does not run agents, discover the targets, build the workspace or measure total-task context or latency.
See [source lookup measurements](project-context-evaluation.md#bounded-source-during-name-lookup) for the exact counts and reproduction.

## Current handoff

Targeted edits start with Author. Pagination, relationship queries and broader discovery load Explore when needed.
History links to `references/recovery.md` only for a pending operation or interrupted lock; existing recovery constraints are preserved there.
Only a lookup explicitly scoped to an existing file supplies that file's insertion handle in `root`.
Unscoped/directory roots cannot substitute, and source changes invalidate the handle. A file map remains the fallback.

The skill checker executes 36 examples. The Rust wrapper example compiles under `deny(missing_docs)` and passes a compiled caller assertion.
It also checks wrong/stale-root refusals, exact undo/redo, unrelated-edit preservation and unchanged index bytes.
The example uses the lookup's source and root directly, without separate source or file-map queries.

`tests/agent-eval/skill-context.json` retains controlled reading costs and the handle-reuse trace audit.
M4n's targeted route costs 2,375 tokens, versus 2,825 recorded skill tokens; this is a conditional 15.9% reduction.
M4n's six-reference route cost 2,960 tokens. Routing adoption has not been tested with fresh agents.
The first regex fr trial has one redundant 387-token map; the second lacks the prerequisite scoped lookup and still needs its map.
`tools/skill-context.py --tokens` reproduces the report using the pinned tokenizer; omit the flag for byte counts without tiktoken.
See [targeted reading measurements](agent-skill.md#targeted-reading-measurement) for scope and the extra cost when recovery is needed.

## Check-report reduction

After reviewing `fr checks`, execution can omit repeated declarations while retaining names, basis, outcomes, diagnostics and unselected names.
The report marks `declarations_omitted: true`; join results to the reviewed listing with the same basis for command metadata.
Default reports and listings retain their fields. Configuration guards and execution behavior stay unchanged.
The portable skill's existing check example combines this option with quiet-success output; all 33 examples pass.
Twelve check CLI scenarios pass, including failed/spawned commands, bounded invalid UTF-8 and reconstruction against the listing.

`tools/checks-context.py` compares real reports on the pinned regex workspace and projects omission onto frozen trial payloads.
Each retained fr trial's four execution reports fall from 2,720 to 1,804 tokens, a 33.7% reduction.
The live comparison preserves tracked source and index bytes; successful output lengths and execution timings can vary.
`tests/agent-eval/checks-context.json` retains actual stdout, fixed-projection counts, binary digest and input provenance.
No new agents ran. These measurements exclude the listing and other task context and do not replace autonomous trial scores.
See [check report measurements](project-checks.md#controlled-report-measurement) for reproduction and boundaries.

## Latest autonomous result

The regex task adds a documented public buffer-writing API by discovering and using an existing workspace helper.
It exercises a real seven-package repository with 227 Rust files and 5,553,380 Rust source bytes.
Only `src/lib.rs` changes, so this is not evidence of coordinated edits across packages.
Four fresh agents ran as two sequential pairs, without conversation history, shared solutions or human task corrections.
No trials failed, restarted or were excluded. Both arms used the same instrumented checks and cooperative tool boundary.

| Repetition | fr context tokens | File context tokens | Result |
|---|---:|---:|---|
| 1 | 11,967 | 6,113 | Both pass |
| 2 | 11,603 | 12,660 | Both pass |

Across both repetitions fr uses 25.6% more measured context.
Both fr agents use exact-name lookup, quiet-success checks and smaller history completion reports without tool failures.
The second file agent keeps verbose check output, accounting for much of the difference between file repetitions.
Skills and transaction reports remain fixed costs; repeated uncached project indexing dominates fr tool time.
These results establish working agent transactions, not general context savings or production latency.

Every trial passes both declared checks in original, changed, undone and redone states.
The checks cover 154 regex/regex-syntax library tests and no-default-feature compilation; integration and documentation tests are excluded.
The independent append/allocation oracle passes 3,174 cases in each project and each clean patch receiver.
Undo/redo restore exact tracked bytes and modes and preserve an unrelated later edit. Project and receiver indexes stay unchanged.
See the [workspace report](agent-workspace-evaluation.md) for raw results, source provenance, runtime details and limitations.

## Durable evidence

Never rewrite existing evidence when changing the tool, skill or evaluator.
Each cohort retains prompts, transcripts, scores, patches and its original skill snapshot, with manifest checksums.

- `tests/agent-eval/results/2026-09-07`: four initial strsim trials; interrupted infrastructure pilots are retained separately.
- `tests/agent-eval/results/2026-09-07-context`: four follow-up strsim trials after M4i; no pilots.
- `tests/agent-eval/results/2026-09-08-regex`: four repeated regex workspace trials; no pilots.

All twelve autonomous trials pass. Recording also supports scored failures; behavioral replay refuses failed trials.
Replay checks recorded patches and transition evidence without rerunning agents. Token auditing recounts retained payloads.
The initial and follow-up strsim findings remain in the [context report](agent-context-followup.md).
Controlled reports are separate under `tests/agent-eval/`: `history-context.json`, `regex/rehearsal.json`, `checks-context.json`, `skill-context.json` and `find-source-context.json`.
The regex rehearsal uses a prescribed solution and rejects three compiled negative controls; it is not autonomous evidence.

Temporary regex sessions remain under `/private/tmp/fr-regex-agent-eval-2026-09-08`, one directory per retained trial name.
The frozen binary is `target/agent-eval-bin/fr-workspace-eval`.
Its SHA-256 is `6eccc6da5db83d295fa447089f6a0337e69a3adf5633df59c4fb60193026e856`.
It, the skills, prompts, evaluator and oracle stayed unchanged throughout both pairs.
Temporary projects are disposable after retention; use repository evidence for replay and audits.

## Validation and commands

M4o's focused project and skill tests pass in `/tmp/fr-m4o-focused.log`, covering 131 project scenarios and 36 executable examples.
The full native/WASM gate passes in `/tmp/fr-m4o-full-check.log`, retaining 311/311 capability coverage and 29 Lean build jobs.
Strict verification passes in `/tmp/fr-m4o-spec-verify.json`: twenty-two fresh anchors and signature maps, zero obligations and a successful Lean build.
The final-binary controlled lookup comparison is `/tmp/fr-m4o-find-context-final.json`; documentation checks pass in `/tmp/fr-m4o-docs-final.log`.
No new formal proof covers the shared page source budget or aggregate query behavior.

M4n's skill workflow and documentation tests pass in `/tmp/fr-m4n-validation.log`, including 37 executed examples and both root-refusal checks.
Final documentation checks pass in `/tmp/fr-m4n-docs-final.log`.
The reading measurement passes in `/tmp/fr-m4n-skill-context.json`; the skill validator, Python compilation and prose checks also pass.
M4n changed no production code; its recorded validation remains historical evidence.

M4m passes the full native/WASM gate in `/tmp/fr-m4m-full-check.log`, including all twelve check CLI scenarios and 33 skill examples.
Capability coverage remains 311/311. Strict verification passes in `/tmp/fr-m4m-spec-verify.json`.
It retains twenty-two fresh source anchors and signature maps, zero obligations and 29 Lean build jobs.
The controlled report passes in `/tmp/fr-m4m-checks-context-final.json`, with the final binary digest checked before and after execution.
Documentation checks pass in `/tmp/fr-m4m-docs-check.log`; formatting, prose budgets and the skill validator also pass.
These are presentation and execution regressions, not a new formal proof of process behavior.

M4l exact token auditing passes in `/tmp/fr-m4l-token-audit.json`.
The opt-in four-patch workspace replay passes in `/tmp/fr-m4l-workspace-replay.log`.
Default acceptance and documentation checks pass in `/tmp/fr-m4l-evidence-check.log`.
They retain sixteen harness regressions, eight earlier patch replays and the controlled history comparison.
Prose budgets remain 272 long sentences and 9,335 Rust comment lines, with other tracked categories at zero.

Use cached dependencies with `CARGO_HOME="$PWD/target/cargo-home"` and `CARGO_NET_OFFLINE=true`.
`tools/check.sh` runs the native/WASM gate; `fr spec verify kernels --json` checks strict source correspondence and Lean builds.
The separate deep audit remains optional unless a change warrants it.
Scoring uses `target/agent-eval-venv/bin/python`, tiktoken 0.12.0 and a checksum-pinned o200k_base vocabulary.
Reference tokens count the prompt and visible instrumented payloads once, excluding system context, hidden reasoning and billed usage.

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-08-regex
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-08-regex
CARGO_HOME="$PWD/target/cargo-home" CARGO_NET_OFFLINE=true cargo test --test agent_acceptance recorded_workspace_patches_pass_checks_oracles_and_exact_reversal -- --ignored
```

Workspace replay needs the pinned regex dependencies. Follow the bootstrap in the workspace report on a cold machine.
The integration regression is opt-in so default CI does not acquire this additional dependency requirement.

## Next steps

Check execution metadata is now optional; use retained traces to reduce remaining project/transaction metadata and skill-loading costs.
Preserve coverage, source bases, guards and reviewable edits.
Validate whether fresh agents adopt the targeted route before claiming autonomous context savings; include a task that actually requires broader exploration.
Measure proposed reductions on fixed transcripts or controlled workflows before requesting another autonomous cohort.
Treat repeated indexing cost separately from returned-context size; these trials explicitly disable the cache.
A later paired task should require coordinated changes across files; the current larger repository task is still a localized facade addition.
Keep portable skill references selective and executable against the distributed binary.
Further authoring operations, M5 automated Lean adoption and M6 framework migrations remain open in [PLAN.md](../PLAN.md).
