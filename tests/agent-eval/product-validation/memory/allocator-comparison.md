# Can the unchanged proof checker return unused memory sooner?

The native rehearsal stopped at 807 MiB against its 768 MiB ceiling. Its
128 MiB reserve means a successful rehearsal must stay at or below 640 MiB.
At that peak, Lean used about 545 MiB, Lake 58 MiB, and the client and
controllers about 204 MiB. The earlier checker-only measurements remain in
[the original diagnostic](README.md); they did not admit another native run.

## One frozen hypothesis

[Lean 4.28.0’s build configuration](https://github.com/leanprover/lean4/blob/v4.28.0/CMakeLists.txt) selects mimalloc 2.2.3. Its documented
[MIMALLOC_PURGE_DELAY setting](https://github.com/microsoft/mimalloc/blob/v2.2.3/readme.md#environment-options)
can return unused pages immediately instead of waiting ten milliseconds.
The prediction is that setting it to zero will reduce resident memory, possibly
at a CPU or latency cost. It cannot reduce memory that the proof still needs.
The implementation may show no useful gain.

[allocator-protocol.json](allocator-protocol.json) freezes this comparison before
measurements. It is a diagnostic, not a new admission policy. Do not change it
in response to the results or retry a stopped case.

## What stays fixed

Use the original qualified seed and reference patch from run 44542024683.
Keep the executable, theorem, imports, package settings, Lean 4.28.0 release,
and complete public checking command unchanged:

    lake -d specs build --wfail

Both conditions use one Lean thread and fresh package outputs. Both remove
ambient mimalloc overrides; only the candidate sets MIMALLOC_PURGE_DELAY=0.
Before measuring, the pinned executable must report the expected allocator
setting in each condition. Unsupported or silently ignored settings stop the
experiment. This probe is setup work, outside the checker-only measurement;
its outputs are retained. The OS file cache is not reset.

Run serially within each Linux and ARM macOS job under the existing limits:
60 seconds, 768 MiB sampled aggregate RSS, 25 sampled CPU seconds, 64 MiB growth
and 128 KiB output per check. Keep the same process sampling and count every
Lake and Lean child process. Neither a different memory metric nor a smaller
import replaces the original check.

## Cases and stopping rules

1. Check the reference with the baseline, then immediate purging.
2. Only if both pass, repeat in the opposite order with fresh package outputs.
3. If every candidate reference check completed, check two candidate controls:
   the original unfinished proof and a wrong proof using True.intro. Both must
   fail for their intended proof error. Resource stops and unrelated errors do
   not count as rejected proofs.

A first-pair failure prevents the second pair. Keep each failure and explicitly
list unstarted cases. A stopped candidate prevents its negative controls.
The other condition in an already-started pair is a distinct comparison; it
may run once. Never rerun the stopped case.

The collector checks that source files remain unchanged during each command.
Negative controls alter only the existing proof region. The report auditor
recomputes RSS and CPU from the retained process samples, verifies output
hashes, and rejects changed conditions, commands, limits, source identities,
verdicts, omitted cases and admission claims.

## Decision

Consider adoption on a platform only if all four positive checks and both
negative controls pass, and the larger candidate peak is at least 10% below
the smaller baseline peak. Publish wall time and CPU alongside memory; passing
this threshold is a checker-only observation, not an agent speedup claim.
A small or inconsistent gain is discarded. Any incomplete comparison remains
inconclusive, with its failures visible.

No result grants native admission. A useful result needs an implementation
integrated into the complete workflow, fresh qualification and all 24 native
rehearsals with their existing reserve. Hosted Linux results cannot admit a
macOS workload. Local execution also remains blocked below 64 GiB free disk.
No live calls or reserved validation sources are used in this comparison.
