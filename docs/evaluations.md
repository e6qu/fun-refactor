# Evaluation evidence

Evaluation artifacts answer narrow questions about fixed revisions and fixtures. They are retained
evidence, not timeless product documentation. Live support comes from `fr audit`, `fr capabilities`
and the command schemas.

## Evidence classes

- Deterministic protocol checks execute fixed workflows without a model.
- Live-agent trials retain prompts, tool events, settings, usage and scores.
- Matched cohorts give two agents the same requested outcomes and compare their exposed context.
- Formal checks prove named model properties under explicit assumptions.
- Compiler, test and behavioral checks establish only the behavior they execute.

Every retained run has a manifest under `tests/agent-eval/results/`. Manifests bind inputs,
evaluators, binaries and reports by digest. Diagnostic runs remain diagnostics after later fixes;
acceptance runs must satisfy their scorer at recording time.

## Current matched results

The accepted 2026-09-17 matched cohort gives both agents seven preview outcomes. The local SDK arm
uses two agent calls and 46,120 input tokens. The direct-file arm uses seven calls and 89,118 input
tokens. Both pass without source mutation, failed commands, bypasses or human correction.

This one pair shows a 48.2% input reduction on its fixed fixture. It does not establish a population
effect, expose hidden reasoning or report billed quota. The immutable
[manifest](../tests/agent-eval/results/2026-09-17-matched-context-acceptance/manifest.json) contains
the exact environment and score.

A separate accepted pair performs one real source-writing task. Both agents change the same Rust
scalar and pass 33 compiled behavior cases without failed calls, direct commands or correction.
The SDK agent uses two exposed calls and 41,389 input tokens. Its retained review produces one
basis-bound write, eight passing check/apply/undo/redo/patch stages and the expected patch. The
direct-file agent uses four exposed calls and 67,985 input tokens. The observed input difference is
-26,596 tokens, or 39.1% fewer for the SDK arm, on this one fixed task. The
[source-writing manifest](../tests/agent-eval/results/2026-09-18-guided-delivery-acceptance/manifest.json)
retains the prompts, requests, responses, Codex settings and usage.

The source-writing pair was repeated after SDK packaging changed the runtime. The accepted repeat
again used Codex CLI 0.154.0, `gpt-5.6-luna` and low effort. Both arms passed the same exact-source
and 33-case compiler oracle with no failed commands or human correction. The SDK arm used two
exposed calls and 63,714 input tokens; the direct-file arm used four calls and 68,098 input tokens.
The observed difference was -4,384 input tokens on this repeat. The
[repeat manifest](../tests/agent-eval/results/2026-09-18-sdk-release-acceptance/manifest.json) is
bound to the new SDK runtime.

The [representative registry](../tests/agent-eval/representative-acceptance.json) joins those two
live matched cohorts to executable deterministic cases for a pinned unfamiliar upstream workspace,
Rust multi-file delivery, TSX/React body changes, CSS/Tailwind/Mermaid surfaces, backend migration
and agent-authored Lean tactics. It records each fixture revision, independent oracle and exact
postconditions. Audit metadata and real execution remain separate: replay runs one worker and
reports missing toolchains as infrastructure failures rather than product failures.

## Find the underlying evidence

| Topic | Retained data or evaluator |
|---|---|
| Completion workflows | [`completion-workflows.json`](../tests/agent-eval/completion-workflows.json), `tools/completion-workflows.py` |
| Guided live-agent completion | `tools/completion-agent-eval.py`, `tests/agent-eval/results/2026-09-17-completion-acceptance/` |
| Matched SDK and direct-file context | `tools/matched-agent-context.py`, `tests/agent-eval/results/2026-09-17-matched-context-acceptance/` |
| Matched SDK and direct-file source writing | `tools/matched-agent-source.py`, `tests/agent-eval/results/2026-09-18-guided-delivery-acceptance/` |
| Representative cross-language acceptance | `tools/representative-acceptance.py`, `tests/agent-eval/representative-acceptance.json` |
| Agent skill reading | `tests/agent-eval/skill-context.json`, `tools/skill-context.py` |
| Compact context protocols | `tests/agent-eval/context-protocol.json`, `tests/agent-eval/context-protocol-v3.json` |
| Semantic bodies, deltas and intents | `tests/agent-eval/semantic-*.json`, `tools/semantic-*.py` |
| Workflow and task packets | `tests/agent-eval/workflow-context.json`, `tests/agent-eval/task-change-context.json` |
| Progressive disclosure | `tests/agent-eval/progressive-disclosure.json`, `tests/agent-eval/progressive-evidence.json` |
| Project caches and construction | `tests/agent-eval/project-*.json`, `tools/project-*.py` |
| External source cohorts | [`PROVENANCE.md`](../tests/agent-eval/PROVENANCE.md), `tests/agent-eval/results/` |

Paths in this table identify families. The manifest for a claimed run is the authority for its
exact files and digests.

## Reproduce and audit

Deterministic reports can regenerate locally. Live Codex runs spend quota and require the evaluator's
explicit confirmation flag. Replay and score retained evidence before considering a fresh run.

```sh
cargo build --features cli
python3 tools/completion-workflows.py --fr target/debug/fr --output /tmp/completion.json
python3 tools/completion-workflows.py --audit /tmp/completion.json
python3 tools/matched-agent-context.py replay tests/agent-eval/results/2026-09-17-matched-context-acceptance
python3 tools/matched-agent-source.py audit tests/agent-eval/results/2026-09-18-guided-delivery-acceptance
python3 tools/representative-acceptance.py audit
python3 tools/representative-acceptance.py replay
```

Use the [Codex runner guide](agent-codex-runner.md) for live-run isolation and the current economical
model configuration. Treat token, byte and call counts as properties of the retained run. Recompute
them after changing prompts, skills, fixtures, model settings or tool output.
