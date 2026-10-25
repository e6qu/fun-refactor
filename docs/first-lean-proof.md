# Write and check your first Lean proof

This lesson starts with a complete Rust function and ends with a saved Lean proof.
Lean is a proof checker. Lake builds a Lean package, which is a group of Lean source files.
A model describes the admitted Rust behavior in Lean.
A theorem states the claim; tactics are the steps used to prove it.

You need an installed fr command and Lean/Lake 4.28.0, matching this fr source.
The generated package pins that Lean release. Follow the [installation guide](../README.md#install) and your [workstation rules](development.md#working-on-a-shared-desktop).
Keep builds and full checks on a runner when sharing a desktop.
This lesson makes no model calls.

## Create the whole example

Create a new empty folder and open a terminal there.
Make a `src` subfolder.
Save this as `Cargo.toml`, the file identifying this Rust project:

```toml
[package]
name = "fr_first_proof"
version = "0.0.0"
edition = "2024"

[workspace]
```

Save this as `src/lib.rs`:

```rust
pub fn keep(value: bool) -> bool {
    value
}
```

A Boolean has two values, `true` and `false`.
This function returns the supplied value unchanged.
The property `identity` asks whether the generated model returns its input for every admitted value.

## Prepare and review the proof task

```sh
fr -C . --json spec init specs
fr -C . --json spec init specs --write
fr -C . --json spec plan src/lib.rs::keep --property identity > formal-plan.json
fr -C . --json spec scaffold --from formal-plan.json --package specs
fr -C . --json spec scaffold --from formal-plan.json --package specs --write
fr -C . --json spec goals specs
```

`-C .` confines fr to this folder. JSON is structured output that an agent or program can read.
A preview shows the files or changes before a write; `--write` applies the reviewed operation.

`init` creates the pinned Lake package under `specs`.
Here `specs` is our chosen package directory; `--package specs` selects it for scaffolding.
The plan describes the current source, its model and the requested property.
Review it before scaffolding the model file.
Scaffolding creates the selected definition and an unfinished theorem.
An unfinished theorem has a proof gap; it is not a completed proof.

For these exact files and names, the generated theorem target is
`specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity`.
The part after `::` names the theorem in the file before it.
The goals report lists its identity and proof state.
For another function, use the target returned for that function instead of changing names by guesswork.

```sh
fr -C . --json spec proof-task specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity
```

Read the requested equality and its assumptions.
A proof task supplies context and empty templates, rather than the proof.
Keep the property fixed while writing its tactics.

## Write tactics and ask Lean to check them

Save this as `proof.lean` in the example folder:

```lean
rfl
```

`rfl` proves an equality whose two sides reduce to the same definition.
It works here because the model returns `value` unchanged.
The file supplies tactics only; fr places them inside the theorem's existing proof region.

```sh
fr -C . --json spec proof-check specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity --from proof.lean
```

Read `passed` in the report. It should be `true`.
The command can return a valid report with `passed: false` when Lean rejects the tactics.
Use its diagnostics to repair the tactics; the successful process exit alone is insufficient.

`proof-check` leaves the model file unchanged.
To see and save the checked proof:

```sh
fr -C . --json spec prove specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity --from proof.lean
fr -C . --json spec prove specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity --from proof.lean --write
fr -C . --json spec check specs --strict --max-debt 0
fr -C . --json spec evidence specs
```

The first command previews the proof-region change.
The write records it in history. Other source and proof regions should retain their bytes.
The strict check requires valid source/signature references and zero unfinished proof gaps.
The evidence command builds the owning Lean package and reports properties, assumptions and remaining obligations.

## What you have established

Lean accepted the identity property of this model.
You supplied its proof tactic in this lesson; an agent independently choosing tactics is a separate result.

The source reference identifies the reviewed Rust function.
It does not establish every part of Rust-to-Lean translation correctness.
For this workflow, `correspondence.proved_implementation_model` remains `false`.
Keep that limit with the result before describing the original program as proved.

After changing the source or the property, obtain and review a fresh task.
Do not renew a source fingerprint to make an old proof appear current.
See the [proof reference](lean-specs.md), [agent proof workflow](agent-formalization.md) and [concrete proof milestone](../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs).
