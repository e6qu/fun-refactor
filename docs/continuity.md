# Development continuity

This is the current contributor handoff. Git history, merged pull requests, the
[changelog](../CHANGELOG.md), [defect ledger](../BUGS.md) and retained evaluator manifests preserve
older milestone detail.

## Current work

[GitHub PR 314](https://github.com/e6qu/fun-refactor/pull/314) contains the matched agent-context
runtime, bounded Lean resource use and the documentation refresh. Its branch is
`matched_agent_context` on the current release mainline. Inspect Git before continuing because the
remote base can advance while the pull request is open.

The Python runtime can now satisfy authored guide actions locally. `GuideInputs` binds exact named
scalar and bounded file values. `complete_guide` refreshes the guide, checks its immutable basis,
refuses write flags, executes every read or preview action and validates each result schema and byte
limit.

Rust, Python and Lean share the guide-binding admission policy. The finite corpus covers counts,
names, bounds, execution flags and bases. Lean proves the admitted-policy implications under the
model definitions, while integration tests check the Rust and Python implementations.

## Product state

`fr` exposes bounded structure and semantic data for 19 parser identities. Agents can inspect code
maps, calls, flow, impact, sources and sinks through progressive Merkle disclosure. They can preview
supported refactors, semantic edits, framework migrations, proof work, declared checks, patches,
reviewed Git actions, undo and redo.

`fr capabilities` is the authority for operation support. The generated matrix currently contains
456 cells, with 311 supported and an explicit reason for each remaining cell. A supported cell may
still refuse an input outside its syntax, identity, confidence or effect contract.

The accepted matched fixture gives both agents seven preview outcomes. The SDK arm uses two exposed
agent calls and 46,120 input tokens. Direct file reads use seven calls and 89,118 tokens. Both arms
pass without mutation, failed commands, bypasses or correction. This is one fixed-fixture result;
the [evaluation guide](evaluations.md) states its limits and points to the immutable manifest.

## Validation

The pull request should pass:

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Lean fan-out defaults to two test jobs and two worker threads per process. Invalid bounds fail before
validation begins. The [development guide](development.md) documents overrides and targeted tests.

## Remaining boundaries

- Generated formalization covers typed pure declarations and structural snapshots. Dynamic,
  effectful, async and unsupported numeric semantics need separate models.
- Lean proves model properties and selected correspondence kernels. Parsing, IR extraction and
  lowering remain trusted or integration-tested unless a specific bridge states otherwise.
- Framework migration admits literal path and JSON behavior plus static intrinsic JSX. Request
  bodies, middleware, authentication, service calls and dynamic rendering refuse.
- Semantic authoring, translation, migration and formalization cover fewer languages than basic
  structural analysis.
- Static analysis preserves uncertainty around reflection, runtime names, external callbacks and
  unresolved dynamic dispatch.
- Live context savings need repetition across projects, models and source-writing tasks.
