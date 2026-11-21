# Immediate purging did not solve proof memory

**Decision: do not adopt `MIMALLOC_PURGE_DELAY=0`.** ARM macOS did not meet the
frozen 10% memory-reduction threshold. Linux's candidate exceeded the existing
768 MiB ceiling. Native admission stays blocked and live attempts remain at 0/96.

The [hosted run](https://github.com/e6qu/fun-refactor/actions/runs/44855003021)
used the [frozen protocol](../allocator-comparison.md) at commit
`0e0e8dded32f36696894c151193923d335557864`. Both pinned executables confirmed
the baseline and candidate allocator settings before measurement. The proof,
imports, Lean release, complete package check and limits were unchanged.

| Platform and condition | Sampled peak RSS | Wall seconds | Sampled CPU seconds | Result |
| --- | ---: | ---: | ---: | --- |
| Linux baseline | 764.32 MiB | 0.75 | 0.48 | Passed |
| Linux immediate purge | 774.67 MiB | 0.35 | 0.32 | Stopped at memory ceiling |
| ARM macOS baseline, first | 599.08 MiB | 6.76 | 1.57 | Passed |
| ARM macOS immediate purge, first | 617.11 MiB | 1.20 | 0.97 | Passed |
| ARM macOS immediate purge, second | 604.72 MiB | 1.37 | 0.93 | Passed |
| ARM macOS baseline, second | 605.66 MiB | 1.28 | 0.79 | Passed |

The macOS candidate's larger peak was 3.01% above the smaller baseline peak;
the protocol required at least 10% below it. Its unfinished-proof and wrong-proof
controls failed for the intended proof errors, at 612.06 and 544.45 MiB respectively.
Correct rejection confirms that these checks ran; it does not establish a saving.

Linux stopped after the first pair. The second pair and both negative controls
remain explicitly unstarted. The stopped case was not retried. Its shorter wall
time is time until termination, not completion or a speedup.

These few sampled checks do not establish that immediate purging caused higher
memory use. Fresh package outputs do not clear the OS file cache, and 100 ms
sampling can miss brief peaks. The first macOS baseline took longer than the
remaining checks; this comparison does not establish a latency improvement.
It does establish that this candidate failed the agreed adoption criterion.

## Retained evidence

- Linux: [original artifact](linux.zip), [summary](linux.json), [GitHub provenance](linux-provenance.json).
- ARM macOS: [original artifact](macos.zip), [summary](macos.json), [GitHub provenance](macos-provenance.json).

The archives preserve their original GitHub bytes and checksums, including the
enclosing `proof-allocator/` directory. CI replays the process samples, checks
the recorded commands, limits, source identities and proof outcomes, and rejects
altered records. No new measurements are needed to review these results.

## What remains

This experiment produced no correction to integrate into the complete workflow.
Do not rerun native admission on its basis. A further attempt needs a different,
measured reduction that preserves the complete proof check and leaves room for
the real client, controllers and 128 MiB reserve. All 24 native rehearsals still
have to pass before live comparisons. The workstation must also satisfy its
independent 64 GiB free-disk requirement.
