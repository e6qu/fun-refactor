# Agent acceptance source and evidence

The accepted 2026-09-20 guided upstream-read trial pins the exact evaluator bytes used during
that run. Its [runner snapshot](runner-snapshots/upstream-read-agent-2026-09-20.py) preserves
those bytes while the active `tools/upstream-read-agent.py` evolves. The representative
acceptance audit checks the snapshot against the recorded digest and keeps the original
runner path in the session and command transcript as historical provenance.
The accepted guided three-file rename has a separate
[runner snapshot](runner-snapshots/upstream-rename-agent-2026-09-20.py) with the same rule: audit
the recorded bytes, not a later edit to the active evaluator.

`strsim-0.11.1.crate` is the unmodified published source archive for rapidfuzz/strsim-rs 0.11.1.
The existing Cargo cache supplied these bytes. No source padding or injected bug modifies the benchmark basis.

- Upstream: https://github.com/rapidfuzz/strsim-rs
- Published archive: https://static.crates.io/crates/strsim/strsim-0.11.1.crate
- SHA-256: `7da8b5736845d9f2fcb837ea5d9e2628564b3b043a70948a3f0b778838c5fb4f`
- The archive's `.cargo_vcs_info.json` records commit `76c5a900e6e12cfc605eee5ab6e36300384c8682`.
- License: MIT; the archive contains the notice, also retained in [LICENSE.strsim](LICENSE.strsim).

This is the complete published crate snapshot, including source, tests, documentation and benchmarks.
It excludes repository files that upstream omitted from the release and contains no upstream Git history.
The archive is evaluation data; no fr library or release target compiles it into the product.
The evaluation extracts it into a temporary directory and invokes its own Rust tests.

The harness adds `.fr/checks.json` with one explicit upstream test command and initializes disposable Git metadata.
It records original tracked bytes and modes before agents start.
The two task prompts request a Unicode Sørensen–Dice fix and a normalized OSA API addition.
Independent oracles live in `tools/agent_eval/oracle.py` outside the agent's permitted inspection surface.

[The initial evidence manifest](results/2026-09-07/manifest.json) binds the retained prompts, instrumented transcripts, scores, patches and skill bundle by SHA-256.
It includes interrupted pilot transcripts and explains their exclusion from scored trials.
The results retain local paths for exact token auditing; those paths do not grant access to live projects.
Git attributes preserve evidence bytes and allow the context-only space lines that exported patches require.
Raw transcripts include excerpts of the MIT source and agent-authored changes.
Behavioral replay needs the archive and standard local tools. Token auditing separately needs the pinned tokenizer and vocabulary.

The [representative acceptance registry](representative-acceptance.json) binds six executable cases:
the pinned unfamiliar regex workspace, Rust multi-file reviewed delivery, TSX/React body changes,
CSS/Tailwind/Markdown/Mermaid surfaces, application migration and agent-authored Lean tactics. Its
replay uses one Cargo and Lean worker and labels a missing toolchain as infrastructure failure.

The [guided upstream read acceptance](results/2026-09-20-upstream-read-acceptance/manifest.json)
adds one fresh `understand`/`trace` Codex run on the pinned regex archive. The evaluator freezes the
binary, admits only bounded guide/follow/find/show requests, verifies the cross-crate call and
checks the agent's answer against source bodies. The retained diagnostics `-diagnostic-1` through
`-diagnostic-4` preserve the failed launch, malformed goal request, and two evaluator contract
failures. None is counted as acceptance. The run exercises no source-writing lifecycle.

The [guided upstream rename acceptance](results/2026-09-20-upstream-rename-acceptance/manifest.json)
uses the same pinned archive and a frozen local `fr` binary. The live agent follows a ready
rename guide, inspects a three-file preview and native review, then executes the unchanged review
with upstream, CLI and minimal-feature checks. The evaluator verifies all eight delivery stages,
exact tracked source changes, patch replay in a fresh archive and 64 independent compiled behavior
cases. The [diagnostic launch](results/2026-09-20-upstream-rename-diagnostic-1/manifest.json)
failed before an agent turn because the outer sandbox blocked Codex CLI initialization. The
successful retry is the sole accepted rename trial; it demonstrates guided cross-file rename, not
authored multi-file body changes.

The [SDK-release matched cohort](results/2026-09-18-sdk-release-acceptance/manifest.json) repeats the
guided scalar source-writing comparison after the Python runtime changed. Both Codex CLI 0.154.0
arms use `gpt-5.6-luna` at low effort, pass exact-source and 33-case compiled behavior oracles and
retain the complete review, reversal and patch lifecycle. Earlier failed fresh attempts were not
recorded as acceptance evidence; their source changes passed, but a malformed JSON transport
request required a retry. The accepted prompt uses the evaluator's bounded `program_lines` form.

The separate [evaluation evidence guide](../../docs/evaluations.md) pins a complete upstream workspace and dependency lock.
It retains four passing autonomous trials and a separate controlled rehearsal.
Its source archive, licenses and lock live under `regex/`; it does not replace the strsim fixture or earlier transcripts.

The [coordinated rehearsal](regex/coordinated-rehearsal.json) reuses this pinned workspace for the two-crate `regex-escape-len` task.
It retains a prescribed three-step authoring batch, exact snapshots, a receiver patch, independent oracles and five rejected implementations.
This supplies task-preparation evidence; it contains no autonomous trials or context comparison.

The [coordinated cohort](results/2026-09-08-coordinated/manifest.json) retains four fresh-agent trials of this two-crate task.
All four pass independent project/receiver oracles and exact reversal checks; the [evaluation evidence guide](../../docs/evaluations.md) explains how to interpret context counts.
Its prompts, transcripts, scores, patches, frozen skills and evaluator fingerprints remain separate from the earlier cohorts and prescribed rehearsals.

The [matched check-output projection](checks-policy-context.json) applies quiet-success policies to each retained coordinated check execution.

The [task-bundle comparison](task-bundle-context.json) is a prescribed generic fixture. It compares
four separate discovery/contract calls with one revision-bound task manifest, binds measurement
sources and the binary by SHA-256, and preserves the same normalized query, target-operation and
check-selection identity. It does not run an agent or mutate source.
It preserves original scores and all other payloads while binding every transformed execution to its recorded payload hash.
The executable measurement rejects stale or inconsistent evidence and checks its transformation against live successful and failing reports.
