# The newer Lean release did not reduce proof memory

**Decision: do not migrate the checker to 4.36.0 on this evidence.** ARM macOS
used more sampled memory with the candidate, and Linux stopped it at the unchanged
768 MiB ceiling. Native admission remains blocked; live attempts remain at 0/96.

The [hosted comparison](https://github.com/e6qu/fun-refactor/actions/runs/46021229454)
ran the [frozen protocol](../toolchain-comparison.md) at
`2e7194a3680669323713a0185af49514af5ac3b0`. Both official distributions were
authenticated before use. The complete package command, source, theorem, proof,
imports and all resource limits were unchanged. Only the selected distribution
changed; this experiment cannot attribute differences to an individual upstream
optimization. No product or historical study toolchain pins changed.

| Platform and condition | Sampled peak MiB | Wall seconds | Sampled CPU seconds | Outcome |
| --- | ---: | ---: | ---: | --- |
| Linux 4.28.0 | 747 | 0.64 | 0.54 | Passed |
| Linux 4.36.0 | 803 | 0.24 | 0.20 | Stopped at memory ceiling |
| ARM macOS 4.28.0, first | 607 | 9.86 | 2.85 | Passed |
| ARM macOS 4.36.0, first | 662 | 9.95 | 2.68 | Passed |
| ARM macOS 4.36.0, second | 646 | 1.00 | 0.73 | Passed |
| ARM macOS 4.28.0, second | 590 | 1.68 | 1.12 | Passed |

The larger macOS candidate peak was 12.22% above the smaller baseline peak.
The protocol required at least 10% below it and no more than 436 MiB, leaving
space for the client/controllers and the existing reserve. Neither criterion
passed. The candidate correctly rejected the unfinished proof and wrong proof
on macOS, using 637 and 597 MiB respectively. This confirms those controls ran,
not a reduction or native admission.

Linux's second pair and negative controls remain explicitly unstarted. The
stopped candidate was not retried. Time until termination is not completion
time. Fresh outputs do not reset OS caches, and 100 ms sampling can miss brief
peaks. These short runs do not establish a latency improvement.

## Evidence

- Linux: [original artifact](linux.zip), [summary](linux.json), [GitHub provenance](linux-provenance.json).
- ARM macOS: [original artifact](macos.zip), [summary](macos.json), [GitHub provenance](macos-provenance.json).

CI replays the original samples, output hashes, commands, identities, limits,
proof outcomes and stopping rules. It also checks each archive against GitHub's
retained digest and its readable summary. A successful audit means the negative
result was recorded correctly; it does not mean that the candidate was useful.

## Review after two unsuccessful reduction experiments

Immediate allocator purging and the newer release both failed to reduce the
obstacle. The import diagnostic already showed a large cost before proof work.
These observations do not establish that another allocator setting or a newer
release will solve it, nor do they prove an unavoidable lower bound.

Keep both candidates out of product defaults. Before another experiment, identify
which retained import data or overlapping processes can be removed without
changing the complete check. Obtain allocation or mapping attribution for that
specific hypothesis; do not cycle through settings, narrow imports, omit package
checking, change the memory metric or raise ceilings. Any eventual correction
still needs full qualification and all 24 native rehearsals. The workstation's
independent disk floor also remains unmet. Continue independent language coverage
while this blocker stays explicit.
