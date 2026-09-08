---
name: lean-spec
description: Write or repair an anchored Lean model for this repository and check source drift with fr. Use for model properties, signature maps or failed Lean checks; not for ordinary code changes.
---

# Lean models with fr

Read `docs/lean-specs.md` for the implemented commands and evidence boundaries.
Inspect the source declaration and `fr spec check --strict` before changing its model.

## State a useful property

Choose a property whose failure would silently change an answer: an inverse, preserved bytes,
operator precedence, a scope invariant or agreement between two implementations.
State its domain and assumptions. Report a false claim rather than weakening it to obtain a proof.

Use `kernels/FrKernels/Edit.lean` and `Position.lean` as working anchor examples.
Strict signature maps currently require Rust source declarations.
`spec extract`, automatic package setup and `SPEC-DEBT` are pending roadmap work.
Author models and initial anchors manually until those features exist.

## Repair drift

Run `fr spec check --strict` and inspect each changed source declaration.
Repair the model or signature map when the source contract changes.
Use `fr spec sync` to preview source-hash renewal and `--write` to apply a reviewed renewal.
Sync updates hashes; it does not synchronize signatures or prove correspondence.

## Check the result

Run `fr spec verify` for strict correspondence and each owning package's `lake build --wfail`.
For these kernels, run `cargo test --test lean_kernels` to compare the shared executable cases.
A finished proof must contain no new `sorry` or unapproved axiom.
If an obligation remains, report it and the failed check explicitly.

Describe whether the result proves a model property, tests implementation correspondence,
or proves implementation correspondence. A source anchor or translation alone proves neither correspondence claim.
