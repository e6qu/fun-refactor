# Development continuity

This document is the current implementation handoff. Git history, merged pull requests,
[CHANGELOG.md](../CHANGELOG.md), [BUGS.md](../BUGS.md) and retained evaluator artifacts preserve
older milestone detail.

## Repository state

PR 40 merged as [GitHub PR 313](https://github.com/e6qu/fun-refactor/pull/313). Its merge head is
`d4b810469b9d5147ecdf9c0d2cfc27b0184e4a21`. The active branch is `matched_agent_context`, based on
that exact commit.

The original roadmap through PR 40 is complete. [PLAN.md](../PLAN.md) now tracks only the finish
line, current boundaries and the active matched-context milestone.

## Delivered product

`fr` is a deterministic agent tool over 19 parser identities. It exposes bounded structural and
semantic project data, progressive Merkle disclosure, code maps, traces, impact, sources and sinks.
Agents can preview and execute supported refactors, semantic edits, framework migrations, formal
proof work, checks, Git patches, reviewed commits, undo and redo without first ingesting whole
source trees.

The main implemented layers are:

- Syntax, symbol, scope, reference, call, flow, impact, entry-point and configuration analysis.
- Project and semantic IR for packages, applications, features, routes, schemas, tests, styles,
  Markdown and Mermaid.
- Checked refactoring, recipe, body-authoring, scalar-intent and structural-delta operations.
- Common application IR for the admitted Next.js, React, FastAPI, Express and Go subsets.
- Content-addressed progressive disclosure with bounded continuations and external object storage.
- Source history, apply, undo, redo, compaction, Git patch export, staging and reviewed commits.
- Declared validation commands with configuration and source-bound receipts.
- Lean kernels, strict source anchors, proof-debt checks and shared Rust/Python/Lean corpora.
- An external-project formalization workbench where the agent authors properties and tactics.
- Native, WASM, browser and zero-dependency Python SDK interfaces.
- `fr audit` and a portable skill that disclose support, boundaries, workflows and proof claims.

`fr capabilities` remains the authority for operation support. The current matrix has 456 cells,
311 supported cells and an explicit reason for each unsupported or inapplicable cell. `BUGS.md` has
no known actionable defect.

## Active milestone

The active milestone completes authored guide execution in the Python SDK and measures it against a
matched direct-file agent.

`GuideInputs` binds exact named scalar and bounded file values. `GuideFile` supplies at most 64 KiB
through a private temporary file that is removed after the preview. `complete_guide` follows every
read or preview action locally. Each action refreshes the guide, checks its immutable basis, rejects
write flags and verifies the returned schema and byte ceiling.

Rust, Python and Lean share the guide-binding admission rule. The 1,024-case finite corpus varies
counts, names, bounds, execution and basis conditions. Lean proves that every admitted binding has
the exact required count and names, stays bounded, disables execution and preserves the basis.
Strict source anchors bind the model to the current Rust declaration.

`tools/matched-agent-context.py` compares the local SDK route with direct file reads over the same
seven preview outcomes. It freezes the binary and records prompts, instrumented requests and
responses, Codex JSONL usage and all internal SDK commands. The baseline requires source-confirming
reads. The scorer rejects missing route or action coverage, unequal outcomes, direct command
bypass, source mutation, failed commands, tool errors,
infrastructure errors and human corrections. Recording a passing acceptance requires an explicit
agent-spend flag; ordinary tests use local fixtures and replay retained evidence.

Two immutable diagnostics preserve failures that improved the handoff and scorer:

- Diagnostic 1 rejected invented SDK imports before the guided arm ran. The file arm invented
  workflow-specific tools and incompatible packet envelopes.
- Diagnostic 2 passed the file arm and reached three guided routes. It exposed extra recipe fields
  and a harness bug that treated an SDK exit code of 1 as the successful-program slot.

The corrected Luna/low cohort passes both arms over the same seven outcomes. The SDK arm runs all
seven routes through 29 recorded internal preview calls while exposing two agent calls. It uses
46,120 input tokens, including 32,000 cached, 1,231 output tokens and 68 reasoning-output tokens.
The direct-file arm uses seven agent calls, 89,118 input tokens, including 74,240 cached, 1,306
output tokens and 217 reasoning-output tokens. Input falls by 42,998 tokens (48.2%); uncached input
falls by 758 tokens (5.1%). Both arms report zero errors, bypasses, mutations and corrections.

This is evidence for one fixed matched pair. It does not establish a population effect, cover
source-writing tasks, expose hidden reasoning or measure billed quota.

## Current validation

The committed SDK slice passes:

- 122 Python pytest cases and `ty`.
- 28 Rust-driven Python SDK tests.
- Strict guide proof verification and the 81-job Lean build.
- The exhaustive Rust/Python/Lean guide-policy comparison.
- Portable-skill validation: 57 examples, 1,228 entry bytes and a largest route of 7,163 bytes under
  the 7,168-byte ceiling.

The matched evaluator has a local end-to-end test, refusal tests and immutable replay checks for its
two diagnostics and accepted pair. Its replay verifies retained file digests, arm classification,
usage differences, call differences and the context-saving classification.

Before review, run the focused evaluator and audit tests, regenerate the source-bound completion
workflow report after the `src/audit.rs` change, then pass `tools/check.sh default`,
`tools/check.sh wasm` and `tools/check.sh deep`. Update this section with the final review head and
results.

## Boundaries that remain

- Generated formalization covers typed pure declarations and structural snapshots. Dynamic,
  effectful, async and unsupported numeric semantics require separate reviewed models.
- Lean proves model properties and selected correspondence kernels. Parsing, IR extraction and
  lowering retain trusted or integration-tested boundaries unless a specific bridge says otherwise.
- Framework migration admits literal path and JSON behavior plus static intrinsic JSX. Request
  bodies, middleware, authentication, service calls and dynamic rendering refuse explicitly.
- Semantic authoring, translation, framework migration and formalization remain narrower than basic
  structural analysis across all languages.
- Static analysis preserves uncertainty around reflection, runtime-generated names, external
  callbacks and unresolved dynamic dispatch.
- The live context saving is one matched fixed-fixture result and needs repetition across projects,
  models and source-writing tasks before any broader claim.
