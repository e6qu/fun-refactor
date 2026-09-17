# Development continuity

This is the current contributor handoff. Git history, merged pull requests, the
[changelog](../CHANGELOG.md), [defect ledger](../BUGS.md) and retained evaluator manifests preserve
older milestone detail.

## Current work

The `guided_source_changes` branch follows merged PR 314. It adds a first-class reviewed guide-run
delivery boundary and a retained live source-writing comparison. Inspect Git before continuing
because the remote base can advance while a pull request is open.

The Python runtime can now satisfy authored guide actions locally. `GuideInputs` binds exact named
scalar and bounded file values. `complete_guide` refreshes the guide, checks its immutable basis,
refuses write flags, executes every read or preview action and validates each result schema and byte
limit. `GuideRun.review()` accepts exactly one task review. `execute_guide` refreshes the guide and
executes only that unchanged review; other writing routes continue through `compile_guided_intent`.

Rust, Python and Lean share the guide-binding admission policy. The finite corpus covers counts,
names, bounds, execution flags and bases. The new delivery kernel also requires a change goal, full
action coverage, one complete review and matching guide identity. Lean proves these model
implications, while finite Rust, Python and Lean cases check implementation agreement.

## Product state

`fr` exposes bounded structure and semantic data for 19 parser identities. Agents can inspect code
maps, calls, flow, impact, sources and sinks through progressive Merkle disclosure. They can preview
supported refactors, semantic edits, framework migrations, proof work, declared checks, patches,
reviewed Git actions, undo and redo.

`fr capabilities` is the authority for operation support. The generated matrix currently contains
456 cells, with 311 supported and an explicit reason for each remaining cell. A supported cell may
still refuse an input outside its syntax, identity, confidence or effect contract.

The accepted preview fixture gives both agents seven outcomes. A second accepted fixture makes one
real Rust source change. The SDK arm uses two calls and 57,464 input tokens; direct files use four
calls and 67,859 tokens. Both pass exact-source and compiled behavior oracles without failures,
bypasses or correction. The SDK result also retains one reviewed write, eight lifecycle stages and
a patch. These are fixed-fixture observations; the [evaluation guide](evaluations.md) points to both
immutable manifests.

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
- Live source-writing evidence now has one accepted scalar task. It still needs repetition across
  projects, models, languages and multi-file change shapes.
