---
name: lean-spec
description: Write, repair or extend a Lean spec for code in this repository. Use when adding a spec, when `fr spec check` reports drift, or when a change to the code has broken the claims about it. Covers what to state and what to leave alone; the lean-prover agent does the proving.
---

# Writing a spec in Lean

`fr spec extract` derives the shape: the types, the signature, the anchor. That part is
decidable and the tool does it. What is left is the judgement, which is deciding what is
worth claiming. That is this skill.

## Read first

- `docs/lean-specs.md` for what the feature promises, and the three things it does not.
- The drift report: `fr spec check`.
- The code the spec is about. A property written without reading the implementation is a
  property about an imagined implementation.

## What to state

State the thing that would be silently wrong. `fr`'s defects have a shape: the answer
looked right and was not. A lowering that drops a bracket, a division that rounds the
other way, a group that leaves out its own symbol. Those are what a spec is for.

Good claims are about a round trip, an invariant, or a disagreement between two paths
that should agree:

```lean
theorem read_write_round_trips (e : Expr) :
    read (write e) = some e
```

Do not state what the type already says. `def f : Nat → Nat` needs no theorem that `f`
returns a `Nat`, and writing one is noise a reader has to get past.

## Anchors

Every spec carries the anchor `fr spec extract` wrote:

```lean
-- fr:spec src/transpile/write.rs::rust_expr @ 8f2c1a9e
```

Do not edit the hash by hand. `fr spec sync` moves it when the signature moves. Editing it
yourself turns drift detection off for that spec, quietly, which is the failure this whole
feature exists to prevent.

## `sorry`

A `sorry` is an obligation with a name, and that is fine. `SPEC-DEBT` counts them and the
count only falls. Leave one where you have stated something true and not yet proved it;
never leave one under a statement you are unsure of. An unproved claim is debt, and a
wrong claim is a trap.

## Proving

Hand each `sorry` to the `lean-prover` agent, one at a time. They are independent, so
they go in parallel. Take no proof that `lake` has not accepted.

If the prover reports that a statement cannot hold, that is a result: either the code is
wrong, or the claim was. Say which, with the evidence. Do not weaken the statement to
close the loop.

## Before finishing

- `fr spec check` reports no drift.
- `lake build` passes.
- Every new `sorry` is accounted for in `SPEC-DEBT`, and the number went down or stayed.
- The record says what the spec claims and what it does not.
