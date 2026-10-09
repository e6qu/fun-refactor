# Separate reply fragmentation from delivery duration

## Design committed before collection

The earlier [streaming comparison](../streaming-2026-10-09/README.md) found that OpenCode
dominated sampled CPU and that short controls could not admit representative work. It changed
fragmentation and pacing together, and Linux counters rounded each process to whole seconds.
This follow-up separates those factors. It is a scripted client diagnostic, not a model trial
or an agent-efficiency comparison. No provider credentials or paid calls are used.

Use the same pinned OpenCode 1.18.34 and fr 0.54.0 binaries, client settings, invoice fixture,
three ordinary-tool responses and exact repair. The two configured provider adapters are Kimi
and GLM; replies come from a loopback provider. Client configuration is held fixed. Do not add a
configuration experiment unless these measurements identify a cause that the setting addresses.

| Condition | Message fragments | Delivery schedule |
| --- | --- | --- |
| whole-burst | One text delta per response | Send immediately |
| whole-paced | One text delta per response | Wait, then send at the chunked schedule's final deadline |
| chunked-burst | Twelve-character text and argument fragments | Send immediately |
| chunked-paced | Same fragments | Absolute two-millisecond deadlines |

The paced modes have the same scheduled final deadline for an identical reply. This controls
total scheduled delivery duration; it does not make intermediate arrival times identical.
Record actual elapsed delivery and maximum scheduling lateness. Socket buffering, backpressure,
worker load and client work can still affect timing. Burst frames can coalesce on the wire;
completed fragmentation comparisons must show more client text-delta events.

The matrix has two platforms (Linux and macOS), two adapters and four delivery conditions.
Each of these sixteen independent cells has two predeclared fresh-client repetitions: at most
32 captures, with no adaptive extension. Each cell runs serially on its hosted worker. The first
failed capture or failed memory-headroom check stops that cell; its remaining repetition stays
unstarted. Other cells have independent budgets and continue. Do not retry failed cells.

Per-capture limits stay at 120 wall seconds, 20 sampled CPU seconds, 768 MiB sampled aggregate
RSS, 16 MiB disk growth and 1 MiB transcript. The qualification target stays at 640 MiB RSS.
Each worker has a ten-minute timeout. Full collection and replay run on GitHub; local work is
limited to small offline checks under the workstation guard.

Linux profiled captures read each process's own user and system ticks from `/proc/PID/stat`,
using `SC_CLK_TCK`; child counters are excluded to avoid double counting. macOS retains its
fractional `ps` counters. Profiles identify counter source and quantum. The same samples enforce
the CPU limit and produce attribution; historical rounded data is not reinterpreted. Process
exits between reads may be missed, and sampled CPU, RSS and process-group containment retain
their existing limitations. Counter or monitor errors stop the worker.

Retain every capture's source/runtime identities, requests, exact replies, submitted source,
delivery timing and process profile. Verify equal replies, runtime and final source across
completed conditions within each platform/adapter group. Report both repetitions and mean CPU
differences for complete pairs only. Failed or incomplete pairs have unknown differences, never
zero. Different conditions run on separate workers, so two repetitions cannot establish a
general causal estimate or performance rate.

## Decision and stopping rule

This diagnostic always reports workstation admission as false. Ordinary-tool results cannot
replace the full ordinary/fr matrix. No local OpenCode execution or live collection follows
from a green diagnostic workflow.

After this single collection, report whether fragmentation, pacing, both or neither are
associated with the observed cost, keeping incomplete comparisons explicit. If the evidence
supports a specific, bounded client fix, commit a separate design before testing it. Otherwise
stop this investigation with local collection blocked; do not create a client optimization
subsystem or broaden the matrix until it passes. The stopped packaging pilot stays stopped.

Entry points are `tools/check-streaming-delivery.py`, `tools/test-streaming-delivery.py` and
the manual collection in `.github/workflows/change-streaming.yml`. Routine PR checks test the
protocol and replay existing frozen evidence; they do not recollect clients.
