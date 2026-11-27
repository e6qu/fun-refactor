# Gruszka under the approved 1 GiB hosted ceiling

The user approved raising the hosted proof-workflow ceiling from 768 MiB to 1 GiB.
The [new protocol](../gruszka-1g-protocol.json) preserves the tools, proof, imports,
commands and all other limits. The [original results](../2026-11-27-gruszka/README.md)
remain unchanged and continue to replay against their original protocol.
Workstation limits and native-client admission remain unchanged.

The [hosted run](https://github.com/e6qu/fun-refactor/actions/runs/46065091046)
measured source `301d3e93aa4646318a29929631e84ad3a5e4ce82`.
The candidate includes Lean compilation, complete export, Gruszka verification,
axiom inspection and the Python driver under one aggregate resource guard.
Tool builds and their Cargo tests run separately during hosted setup.
Linux has completed. The new ARM macOS job remains queued for a hosted runner;
its earlier successful 768 MiB results do not stand in for a new measurement.

| Platform and condition | Sampled aggregate RSS | Wall time | Outcome |
| --- | ---: | ---: | --- |
| Linux baseline, first | 783.2 MiB | 0.74 s | Passed |
| Linux complete candidate, first | 809.2 MiB | 12.94 s | Passed |
| Linux complete candidate, reverse | 805.9 MiB | 13.02 s | Passed |
| Linux baseline, reverse | 740.7 MiB | 0.74 s | Passed |

Linux completed both full proof checks and rejected the unfinished and incorrect source proofs.
Gruszka accepted the complete export, and the target theorem used only `Quot.sound`
and `propext`. No Linux case stopped or remained unstarted.
The maximum candidate RSS was about 215 MiB below the new ceiling.
Raising the ceiling resolved the earlier Linux memory stop; it did not reduce memory consumption.

The decision now reports `completed-within-limits` separately from
`original_reduction_outcome`. The latter still reports `no-usable-reduction` for Linux:
the original 436 MiB target and 10% reduction did not occur.
That historical target does not reject completion under the newly approved ceiling.
Gruszka remains an independent kernel after Lean elaboration; this does not qualify
a complete compiler migration or native-agent admission.

The ZIP retains the original Linux GitHub artifact. Its adjacent JSON file binds its artifact ID,
hashes, run and source commit. Replay checks both protocol versions and rejects
attempts to reinterpret earlier records under the new ceiling.
Run `product-memory.yml` with `experiment=gruszka-audit` for replay without new measurements.
