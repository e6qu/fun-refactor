# Scripted streaming CPU comparison

## Result and decision

[Hosted run 37900153955](https://github.com/e6qu/fun-refactor/actions/runs/37900153955)
collected and verified all eight cases from commit `d63ddaf2`. Of 32 planned captures,
10 completed, 6 hit the CPU limit and 16 remained unstarted. Only the two Linux ordinary-tool
cases completed both comparison pairs. None of the macOS cases qualified for admission.

The names below identify configured provider adapters receiving scripted replies, not live models.
CPU values are sampled process-group seconds. A dash means the run never started.

| Platform / adapter / tools | Whole replies | Chunked replies | Admission |
| --- | --- | --- | --- |
| macOS / Kimi / ordinary | 9.31 | stopped at 20.04 | refused |
| macOS / Kimi / fr | — | stopped at 20.03 | refused |
| macOS / GLM / ordinary | 13.84 | stopped at 20.07 | refused |
| macOS / GLM / fr | — | stopped at 20.07 | refused |
| Linux / Kimi / ordinary | 10, 11 | 14, 15 | passed |
| Linux / Kimi / fr | — | stopped at 21 | refused |
| Linux / GLM / ordinary | 12, 12 | 17, 16 | passed |
| Linux / GLM / fr | — | stopped at 21 | refused |

On Linux, the complete ordinary-tool comparisons added 4.0 and 4.5 mean sampled CPU seconds
with chunked delivery. Replies and exact edited source matched. Each completed chunked capture
produced 759 native text-delta events, compared with three for whole delivery.

On macOS, OpenCode accounted for 19.08–19.32 of the 20.03–20.07 CPU seconds in the stopped
streamed captures; Python accounted for 0.72–0.97 seconds. The fr processes in the two fr cases
accounted for 0.01 and 0.02 seconds. Peak sampled RSS in those four captures was 647.55–656.17 MiB,
also above the 640 MiB admission target. This identifies the client as the dominant sampled
consumer in these controls. It does not diagnose which client subsystem caused the cost.

Linux `ps` CPU counters in this run have whole-second precision per process; a displayed zero
does not mean no CPU use. The 21-second stop observations reflect sampling and counter granularity,
not a raised limit. Fragmentation and pacing both change in this experiment. Two complete pairs
cannot establish a general performance rate, and stopped pairs have no calculated mean difference.
No efficiency or historical live-pilot root-cause claim follows from this scripted experiment.

The repair is to admission: `check-change-workstation.py` now requires complete, replayed streaming
evidence on both platforms, matching current runtime and macOS executables, before starting any
local client. The short controls alone cannot admit another workstation capture. This collection
does not qualify, and no workstation or live model call followed it. All limits remain unchanged.

The [provenance](runs/37900153955/provenance.json) binds GitHub artifact IDs and archive digests;
the [results](runs/37900153955/results.json) preserve each attempt. Eight original archives plus
the compressed frozen Python runner occupy about 1.03 MiB. Run `tools/check-streaming-evidence.py`
to replay them without OpenCode, fr execution or model calls. CI performs the full replay; use the
workstation guard for any local check. Routine PR checks no longer recollect the experiment.

## Committed design

This diagnostic tests whether incremental reply delivery adds enough client CPU work to explain
a resource-admission gap. It cannot attribute the stopped live packaging attempt retroactively,
and it does not compare model quality or agent efficiency. No provider credentials or paid calls
are used. The configured provider adapters talk only to a scripted loopback endpoint.

The design is committed before collection. Each Linux/macOS, Kimi/GLM adapter and ordinary/fr tool
combination gets four fresh client captures: whole/chunked/chunked/whole for ordinary tools and
chunked/whole/whole/chunked for fr tools. These are predetermined repetitions, not retries.
The first failed capture or failed memory-headroom check ends that case; remaining captures stay
unstarted. Other matrix cases have independent budgets. No stopped live collection is resumed.

Both modes receive identical source, task, explanatory text and tool actions. The fixture reads
an invoice module and changes its total to exclude voided records. The ordinary route replaces
source; the fr route explores, inspects behavior, previews and applies a body change. Exact final
source bytes and replayed tool accounting must agree. Opaque workspace handles are normalized
only when comparing reply identities; their original values remain in the raw evidence.

Whole mode sends one message delta. Chunked mode splits text and JSON arguments into twelve-character
pieces, flushes each SSE frame and waits two milliseconds between frames. Unicode, quoting,
multiple calls, finish reasons and usage preservation have offline tests. Actual client delta
counts must increase before a complete comparison is accepted. Pacing deliberately adds wall
time, so elapsed-time differences cannot be interpreted as a pure processing penalty.

The experiment uses OpenCode 1.18.34 and fr 0.54.0 with the same pinned release archives as the
existing change controls. Client settings remain low reasoning, 32,768 context, 2,048 output,
small-heap hints and disabled JIT. Usage returned by the scripted provider is synthetic.

Each capture retains its frozen task/runtime/executable identities, provider requests and replies,
native events, tool calls, submission if completed, process samples and resource outcome.
The existing process sampler supplies CPU totals by executable and aggregate RSS. Sampled values
can miss short peaks or escaped process groups; shared pages can be counted more than once.

Limits remain 120 wall seconds, 20 CPU seconds, 768 MiB sampled RSS, 16 MiB disk growth and
1 MiB capture transcript. Memory admission requires at most 640 MiB. Each hosted job has a
ten-minute timeout, uploads failures too and replays its report offline. A green workflow means
the measurements were collected and verified; consult `admitted` for resource admission.

Entry points: `tools/check-change-streaming.py`, `tools/test-change-streaming.py` and
`.github/workflows/change-streaming.yml`. Run client captures only on GitHub. Do not rerun failed
captures locally or raise resource limits. Measurements determine whether a further fix is justified.
