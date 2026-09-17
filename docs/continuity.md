# Development continuity

This is the short contributor handoff. The [roadmap](../PLAN.md) owns unfinished outcomes; the
[changelog](../CHANGELOG.md), [defect ledger](../BUGS.md), retained evaluator manifests and Git
history preserve completed detail.

## Baseline

PR 316 completed first-class execution of one unchanged reviewed guide run and retained the first
matched live source-writing cohort. The merged system provides:

- Bounded project, semantic, application and evidence models for 19 parser identities.
- Merkle-addressed progressive disclosure and a Python object-store protocol.
- Structured goals, ten guide routes and native intent compilation.
- Built-in refactors, recipes, semantic and surface edits, application migration and proof work.
- Checks, history, apply, undo, redo, recovery, patches and reviewed Git operations.
- Lean admission and transition kernels with strict source anchors and shared executable cases.
- A portable agent skill and zero-dependency typed Python runtime.

Use `fr audit` and `fr capabilities` for live counts and support. A supported route can still refuse
an input outside its syntax, identity, confidence or effect contract.

## Active outcome

The first bulk roadmap item resets documentation around the merged product and its remaining work.
It removes stale milestone prose, shortens the user entry path, cross-links authoritative guides and
corrects inconsistencies found while dogfooding `fr`.

The next implementation outcome is uniform reviewed guide delivery across every writable route.
Today, native tagged intents can review and execute those routes, while the simpler
`complete_guide`/`execute_guide` path accepts only a run containing one task review.

Dogfooding this outcome fixed three defects: the Python guide bridge now accepts the
`application-migration` operation advertised by its route, path-scoped symbol queries restrict the
scan before parsing and resolution, and the Lean grammar reads the common proof forms listed in
B934. The current outcome adds bounded structural branches for `cases`, `induction` and `rcases`
without hiding tactics behind an opaque fallback.

## Validation

Run focused tests while editing, then:

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Rust test fan-out for the default, deep and Lean-kernel gates defaults to two, as do Lean worker
threads. The [development guide](development.md) documents one-worker overrides and targeted gates.

For repository changes, dogfood `fr`, recipes and reviewed history whenever an admitted operation
exists. Treat an incorrect preview, refusal or impractical flow as product evidence and fix its root
cause in the same outcome. Record direct editing only when no suitable `fr` operation exists.

## Durable boundaries

- Generated formalization covers admitted typed pure declarations and structural snapshots.
- Parsing, extraction, lowering, hashing, Git, filesystems and runtime behavior remain trusted or
  separately tested unless a proof record states a narrower correspondence claim.
- Application migration currently models literal JSON routes, path JSON routes and static
  React/Next components. Other effects require explicit IR semantics.
- Static analysis retains uncertainty around reflection, runtime names, external callbacks and
  unresolved dynamic dispatch.
- Live source-writing evidence covers one accepted scalar task and needs cross-project,
  cross-language, multi-file, migration and proof-writing cohorts.
