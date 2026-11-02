---
name: lean-spec
description: Write or repair an anchored Lean model for this repository and check source drift with fr. Use for model properties, signature maps or failed Lean checks; not for ordinary code changes.
---

# Lean models with fr

Read [Lean specifications](../../../docs/lean-specs.md) for the implemented commands and evidence boundaries.
For proof steps, use the shared [fr skill](../../../skills/fr/SKILL.md) and
[interactive Lean guide](../../../skills/fr/references/lean.md).
Inspect the source declaration before changing its mathematical model.
In commands below, replace `<PACKAGE>` with the owning Lean package directory, such as `specs` or `kernels`.
Run `fr --json spec check '<PACKAGE>' --strict --max-debt 0` to inspect source drift and unfinished proofs.

## State a useful property

Choose a property whose failure would silently change an answer: an inverse, preserved bytes,
operator precedence, a scope invariant or agreement between two implementations.
State its domain and assumptions. Report a false claim rather than weakening it to obtain a proof.

Use [Edit](../../../kernels/FrKernels/Edit.lean) and [Position](../../../kernels/FrKernels/Position.lean) as working anchor examples.
Strict signature maps currently require Rust source declarations.
Package initialization and scaffolding exist. The [complete first-proof lesson](../../../docs/first-lean-proof.md)
shows `spec init`, `property-task`, `plan` and `scaffold`, with every input included.
Review the generated definition and claim. Write a manual model when the source form needs one;
do not claim that scaffolding proves the source and model equivalent.

## Repair drift

Run `fr --json spec check '<PACKAGE>' --strict --max-debt 0` and inspect each changed source declaration.
Repair the model or signature map when the source contract changes.
Use `fr --json spec sync '<PACKAGE>'` to preview source-hash renewal and `--write` to apply a reviewed renewal.
Sync updates hashes; it does not synchronize signatures or prove correspondence.

## Check the result

Run `fr --json spec verify '<PACKAGE>'` for strict anchor and signature checks plus package builds.
For the example kernels, `cargo test --test lean_kernels` compares shared executable cases.
Follow the project's resource rules: full builds and complete gates belong on GitHub runners.
A finished proof must contain no new `sorry` or unapproved axiom.
If an obligation remains, report it and the failed check explicitly.

Describe whether the result proves a model property, tests implementation correspondence,
or proves implementation correspondence. A source anchor or translation alone proves neither correspondence claim.
