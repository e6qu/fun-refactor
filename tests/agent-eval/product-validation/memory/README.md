# Proof memory before live work

The [original native stop](native-stop.json) retains six passing Codex non-proof
controls and the proof control that exceeded the 768 MiB ceiling. Lean accounted
for about 545 MiB of the 807 MiB sampled peak. The sequence stopped with 17 controls
unstarted and no live calls. The 128 MiB admission reserve remains required.

The hosted diagnostic asks which part of that check needs the memory. It uses the
original qualified seed and reference patch from run
[44542024683](https://github.com/e6qu/fun-refactor/actions/runs/44542024683):

- Build the unchanged proof package with the original public checking command.
- Check the same proof source directly with Lean, to isolate Lake overhead.
- Check a separate trivial example importing `Std`, to isolate import cost.
- Check the same trivial example importing `Init`, as a smaller import control.

Run these serially on Linux and ARM macOS with the same pinned Lean 4.28.0 release
and the existing 60-second, 768 MiB, 25 CPU-second, 64 MiB growth and 128 KiB output
limits. Use fresh package outputs for each case. This does not reset the OS file
cache. Repeat once only if all four cases complete; never retry a stopped case.
Keep per-process samples and every failure.

These are checker-only measurements. They omit the native client and cannot grant
admission, close the live comparison or establish a product speedup. A smaller
import control cannot replace the task's protected imports. A direct proof check
cannot replace the required complete package check. Linux results cannot admit a
macOS workload.

If the unchanged checker cannot leave enough room for the client and admission
reserve, keep local execution blocked. Do not raise limits, narrow the claim,
change the task imports or hide checking outside the complete-attempt measurement.

## First hosted result

The [Linux job](https://github.com/e6qu/fun-refactor/actions/runs/44560547064/job/135688313793)
completed both repetitions. The original [artifact](2026-11-19/linux.zip),
[summary](2026-11-19/linux.json) and [download provenance](2026-11-19/linux-provenance.json)
are retained. Both platform jobs and the final retry have now completed.

| Checker-only case | Sampled peak RSS across two repetitions |
| --- | ---: |
| Original package build | 758–764 MiB |
| Same proof checked directly | 659 MiB |
| Trivial example importing Std | 562–659 MiB |
| Trivial example importing Init | 407–415 MiB |

Every Linux case completed within the diagnostic limits. Even direct checking of
the unchanged proof exceeded the 640 MiB available after the admission reserve,
without a client. Package checking used almost the entire 768 MiB ceiling.
The import controls indicate a substantial import cost, but these short runs use
100 ms sampling and can miss brief peaks. They do not isolate exact heap costs.

**Decision:** keep native proof admission blocked. This diagnostic has not shown
a correction that preserves the complete task, checking requirements and limits.
Do not start the live comparison or substitute the smaller import control.
The [final hosted run](https://github.com/e6qu/fun-refactor/actions/runs/44564485493)
retains both platforms, including a Linux package-check memory refusal:

| Final checker-only case | ARM macOS sampled peak | Linux sampled peak |
| --- | ---: | ---: |
| Original package build | 589–605 MiB | 755 MiB completed; 770 MiB stopped |
| Same proof checked directly | 538–548 MiB | 659–667 MiB |
| Trivial example importing Std | 509 MiB | 534–659 MiB |
| Trivial example importing Init | 308–335 MiB | 397–412 MiB |

The [original macOS artifact](2026-11-19/macos-first.zip),
[final macOS artifact](2026-11-19/macos-final.zip) and
[final Linux artifact](2026-11-19/linux-final.zip) keep all samples and command outputs.
Each has a neighbouring JSON summary and provenance record with its original GitHub checksum.
The macOS Lean executable hash matches the original workstation admission record.
The checker leaves only about 35–51 MiB below the 640 MiB admission budget after its
reserve, before adding the client and controllers. The original full native check
already exceeded the ceiling. Keep the block; these measurements do not admit a retry.
No live calls or reserved validation sources were used.

Replay the retained samples without running Lean:

```sh
PYTHONPATH=tools python3 -B -m agent_eval.product_memory audit tests/agent-eval/product-validation/memory/2026-11-19/linux.zip
```

On the workstation, run this command through the required local guard. The audit
checks every case, resource verdict, process sample and output hash. It refuses
changed limits, missing cases, retried stops and any claim of native admission.
