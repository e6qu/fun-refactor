# Scripted streaming CPU comparison

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
