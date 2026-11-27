# Gruszka: compatible on ARM macOS, no complete memory reduction

The [hosted experiment](https://github.com/e6qu/fun-refactor/actions/runs/46061804669)
used the frozen [protocol](../gruszka-protocol.json) and [comparison design](../gruszka-comparison.md).
It kept Lean 4.28.0, the original proof, imports, package command and resource limits.
The measured source is `457c5e50f137325f97412d68a539c1019171e3bb`.
No native clients or live model calls ran. Product defaults remain unchanged.

| Platform and condition | Sampled aggregate RSS | Wall time | Outcome |
| --- | ---: | ---: | --- |
| Linux baseline | 764.3 MiB | 0.65 s | Passed |
| Linux complete candidate | 795.1 MiB | 0.56 s | Stopped during package build |
| ARM macOS baseline, first | 600.9 MiB | 6.61 s | Passed |
| ARM macOS complete candidate, first | 664.4 MiB | 16.21 s | Passed |
| ARM macOS complete candidate, reverse | 660.5 MiB | 16.47 s | Passed |
| ARM macOS baseline, reverse | 589.5 MiB | 1.36 s | Passed |

Linux exceeded the unchanged 768 MiB ceiling before export or Gruszka started.
At the stopping sample, the candidate included about 40 MiB of Python driver memory,
111 MiB of Lake and 644 MiB of Lean. This does not measure Gruszka's Linux memory use.
The reverse pair and negative controls were not started, and the stopped case was not retried.

ARM macOS completed both full checks and rejected the unfinished and incorrect source proofs.
The exporter decoded 912 modules and 87,540 declarations, producing about 51 MiB of Susz data.
Gruszka checked the entire export with one worker and returned `accepted` both times.
The target theorem depended on `Quot.sound` and `propext`, within the frozen axiom policy.
The source rejection controls failed in Lean before export; they do not qualify
Gruszka against malformed or incorrect exported declarations.

The macOS process traces show Gruszka itself peaked at 218.6–220.7 MiB sampled RSS.
The exporter peaked at 531.5–561.9 MiB. These are per-process observations from the
complete runs, not separate isolated benchmarks. Overall candidate peaks occurred
while Python, Lake and Lean were running together. The Python driver contributed
43.7–47.9 MiB at those peaks. Removing that overhead alone would not meet 436 MiB.
The original baseline also exceeded that checker budget on both platforms.

The complete macOS candidate missed both the 10% reduction requirement and 436 MiB budget.
Lean still has to elaborate source and execute tactics before Gruszka can read a proof.
This integration adds independent verification; it does not replace Lean's compiler
or eliminate its import-memory cost. A full switch is not qualified by these results.
Further reduction work needs allocation or mapping evidence for the retained Lean
frontend and exporter, rather than another unchanged run or a larger resource limit.

## Retained evidence and replay

The two ZIPs are the original GitHub artifacts. Their adjacent JSON files bind
artifact ID, SHA-256, run and source commit. Each contains the measured summaries,
guard records, process traces, output and tool provenance. The replay checks hashes,
protocol, commands, case order, resource decisions and provenance agreement.
It rejects unknown archive members and altered records without building any tools.

Run `product-memory.yml` with `experiment=gruszka-audit` on GitHub to replay.
Tool setup built the three pinned Rust repositories with their lockfiles and ran
their Cargo tests on both platforms. This does not establish complete upstream
adversarial qualification. Production use still needs the statement/export and
malformed-export checks described in the comparison design.

The initial [harness preflight](https://github.com/e6qu/fun-refactor/actions/runs/46061662316)
failed before any measurements: a mutation test exposed a decision that trusted
the stored pass flag without checking the guard's stop reason. That defect was fixed
before the measured run. Its failed outcome remains visible.
