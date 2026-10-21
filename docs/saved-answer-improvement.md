# Spend less time reopening saved answers

A user asks where `glob_with` appears and what indexed code its change could affect.
Python programs use the helper library, called the SDK, to call `fr` and keep saved answers.
On reopening that unchanged question, the previous helpers repeatedly hashed the same saved contents.
A content hash identifies a piece by its bytes. Parent pieces record the hashes of their children.
The new code checks each distinct piece once and builds parent hashes from those verified child hashes.
It still rejects missing, corrupted or cyclic data and returns independently editable copies of repeated values.

The comparison uses the previous SDK from merged PR #553 and the changed SDK with the same release build of `fr`.
Both answer the same questions on identical pinned glob, Boltons and JMESPath sources.
Each route starts a fresh Python worker per question and keeps its own explicitly named native cache.
The saved route also stores its task plan and answer, validates dependencies before reuse, and repeats discovery after an edit.
The task plan records which inputs an answer depends on, so edits can expire the answer.
The fresh route asks `fr` again and retains no task plan or answer.

Each session contains first use, unchanged reuse and one edit, with three repetitions per project and SDK.
SDK order alternates by project. Saved and fresh order alternates by repetition within each SDK.
Each SDK finishes its three repetitions on a project before the other SDK starts that project.
The edit replaces the selected name in one file through `fr file replace`; it checks invalidation rather than delivering a complete rename.
All 108 request answers agree between SDKs, and each saved answer agrees with its matching fresh check.

## Complete session time

Median seconds include Python startup, validation, discovery, persistence, output, checkout copies, the edit and cleanup.
Preparation and environment setup remain separate in the raw reports and recorded workflow steps.

| Project | Previous saved | Current saved | Change | Previous fresh | Current fresh | Current saved overhead versus fresh |
|---|---:|---:|---:|---:|---:|---:|
| boltons | 2.413 | 2.230 | -7.6% | 1.740 | 1.758 | +26.9% |
| glob | 2.922 | 1.935 | -33.8% | 0.688 | 0.703 | +175.1% |
| jmespath | 4.420 | 2.952 | -33.2% | 0.932 | 0.928 | +218.0% |

A negative change means less time. Positive overhead means saving still costs more than asking again.

## Unchanged requests, CPU, memory and storage

CPU below sums worker and child CPU across the three questions; it excludes copy, edit and cleanup CPU.
The raw reports retain GNU time CPU, edit CPU and separate preparation measurements.
Memory is the largest sampled total of resident memory across the worker and its child processes.
This measure, RSS, counts memory present in RAM. Sampling can miss short peaks.
Stored bytes count file contents; directory and filesystem block allocation add unmeasured overhead.

| Project / SDK | Saved reuse seconds | Saved request CPU seconds | Maximum memory MiB | Final saved KiB |
|---|---:|---:|---:|---:|
| boltons / current | 0.219 | 1.895 | 75.0 | 100.6 |
| boltons / previous | 0.342 | 2.095 | 75.0 | 100.6 |
| glob / current | 0.309 | 1.169 | 49.5 | 313.5 |
| glob / previous | 0.925 | 2.164 | 51.5 | 313.5 |
| jmespath / current | 0.352 | 1.798 | 56.3 | 548.3 |
| jmespath / previous | 1.035 | 3.240 | 58.7 | 548.3 |

## What this establishes

The dedicated profile locates repeated hashing and also exposes the cost of flushing many small files to disk.
For example, the original glob answer contains 1,387 distinct stored pieces.
Each new file write waits for its bytes to reach disk before finishing. That storage cost remains after the hashing fix.
Profile timings include profiler overhead and do not enter the timing tables above.
The change preserves object contents, content addresses, verification and the existing resource limits.
It adds no alternate reader, fallback or compatibility layer.

Both measured routes use Python, so this experiment does not isolate Python overhead against direct command-line use.
Three repetitions and uncontrolled operating-system file caches limit the precision of the timing estimates.
The requests preserve bounded, missing and ambiguous answers. They establish no general agent, model-token or billing savings.

All projects in this follow-up are familiar. The original first JMESPath evaluation remains separate and unchanged.
The recurring checks audit that original result beside its original code and audit these measurements beside the current code.

The next cost improvement should address the remaining measured storage and process costs and retain complete session comparisons.
Broader agent-cost trials, translation and proofs connected to source code remain separate work.

[Matched measurement and raw archives](../tests/agent-eval/saved-answer-improvement/record.json) | [Original costs](saved-work-cost-results.md) | [Authenticated profiles and workflow steps](../tests/agent-eval/saved-answer-improvement/publication.json)
