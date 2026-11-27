# Evaluate Gruszka on the complete proof workflow

The user selected Gruszka after reviewing alternative Lean implementations.
This experiment tests its fit before changing product defaults.
It does not admit native clients or authorize a live comparison.

## Candidate and hypothesis

[Gruszka](https://github.com/jasisz/gruszka) is an external kernel written in Rust.
It checks compiled proof declarations in Susz format.
Its compact representation and delayed decoding may reduce independent replay costs.
It cannot elaborate the source or execute its tactics.
The existing import-memory cost may therefore remain the limiting factor.

[susz-olean](https://github.com/jasisz/susz-olean) reads the pinned Lean 4.28 files
and writes Susz directly. This avoids a large intermediate NDJSON file.
The exporter and kernel use one worker each. All declarations are checked;
Gruszka's `--only` mode is excluded because it admits earlier declarations unchecked.

The [protocol](gruszka-protocol.json) pins the three Rust repositories, compiler,
Lean archives, original qualification and resource limits before measurement.
Each tool is built with its committed Cargo lockfile.
The collector retains binary hashes, lockfile hashes and the Rust compiler identity.

## What is measured

The baseline runs `lake -d specs build --wfail` on the qualified reference proof.
The candidate runs that same package command, exports the complete `FrSpecs`
environment, checks it with Gruszka and inspects the target theorem's axioms.
The target is `Product.clamp_bounds`. Only `Classical.choice`, `Quot.sound`
and `propext` may occur in its axiom dependencies.

One guard covers the entire candidate process tree and all four stages.
Export, verification and the Python stage driver count toward memory and time.
Generated exports count toward disk use. There is no precomputed proof export.
Tool installation and compilation are setup costs recorded separately by the workflow.
Fresh package outputs do not imply cold operating-system caches.

The first pair is baseline, then candidate. A successful pair permits the reverse order.
A stopped condition is retained and is not retried. If the candidate references pass,
unfinished and incorrect source proofs must fail under the same limits.
Those controls test source rejection, not independent malformed-export rejection.

The original ceiling remains 768 MiB. A useful result needs at least a 10% reduction
and no more than 436 MiB for the complete candidate pipeline.
The latter leaves the original client/controller allowance and reserve intact.
Resource samples can miss short peaks and double-count shared pages.

## Interpretation and migration boundary

Passing this experiment establishes limited compatibility on one qualified task.
It does not prove exporter equivalence or replace the source elaborator.
Gruszka accepts axioms as assumptions, so kernel acceptance alone cannot establish
that an exported theorem matches the source claim with its intended dependencies.
A production migration also needs independent statement and export comparisons,
malformed-export controls, all proof-authoring operations and package qualification.
Any native admission still requires all 24 complete rehearsals.

The experiment does not change toolchain pins, proofs, imports or package commands.
Historical failures remain evidence. A complete switch needs a measured benefit
across the source-to-delivery workflow, including the remaining Lean frontend.

Run on GitHub with `product-memory.yml`, selecting `experiment=gruszka`.
Select `experiment=gruszka-audit` to replay tests without new measurements.
Pull requests replay the harness tests without starting new measurements.
