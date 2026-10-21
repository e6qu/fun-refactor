# Save an answer with one database transaction

A user asks where `glob_with` appears and which code a change could affect.
The Python helper library, called the SDK, can save that answer and the inputs it depends on.
A content fingerprint identifies each distinct piece of the answer by its bytes.
JSON is the text format used to save those pieces, including numbers, lists and named fields.
Previously, saving the glob answer wrote 1,387 files and waited for each write to reach disk.
The local store now puts the pieces in one SQLite database inside the chosen directory.
SQLite is a database library included with Python; it needs no separate server.
A transaction commits the pieces together. If it fails, none of its changes become visible.
Disk synchronization stays enabled. Conflicting bytes and corrupted records still fail verification.

## What is compared

The previous SDK is the code merged in PR #554. Both SDKs use the same optimized `fr` build on the same runner.
Both ask identical questions on pinned glob, Boltons and JMESPath sources at the same absolute project path.
glob matches file paths in Rust. Boltons provides Python utilities. JMESPath selects values from JSON documents.
Each project and SDK has three sessions containing first use, unchanged reuse and one source edit.
Each question starts a new Python process. Both approaches keep their own initially empty native analysis cache.
The saved approach also keeps its answer and a task plan, which records the inputs needed for the answer to remain valid.
The fresh approach asks `fr` again without saving a task plan or answer.
After an edit, the saved approach checks those inputs, expires the old answer and repeats discovery.
The edit uses `fr file replace` in one file. For example, glob replaces `glob_with` with `glob_with_cost_renamed`.
It then asks about the old name. This checks whether an answer expires; it does not deliver a complete project rename.
SDK order alternates by project. Saved and fresh order alternates by repetition within each SDK.
Each SDK finishes its three repetitions on a project before the other SDK starts that project.
All 108 request answers agree across SDKs, including missing, ambiguous and bounded results.

## Complete session time

Median seconds include Python startup, validation, discovery, saving, output, source copies, the edit and cleanup.
Builds, dependency installation and initial downloads remain separate in raw reports and workflow steps.

| Project | Previous saved | Current saved | Change | Previous fresh | Current fresh | Current saved overhead versus fresh |
|---|---:|---:|---:|---:|---:|---:|
| boltons | 4.017 | 3.242 | -19.3% | 3.099 | 3.190 | +1.6% |
| glob | 3.588 | 1.788 | -50.2% | 1.186 | 1.266 | +41.3% |
| jmespath | 4.743 | 2.214 | -53.3% | 1.606 | 1.723 | +28.5% |

Negative change means less time than the previous saved approach.
Positive overhead means saving costs more than asking again with the current SDK.
Percentages compare the median complete session times above.

## CPU, memory and saved bytes

CPU counts processor work in Python and its `fr` child processes. Elapsed time also includes waiting.
Request CPU sums the three questions, sampled before the final report and answer writes.
Raw GNU time values include those writes and round CPU to 0.01 seconds. Copy, edit and cleanup CPU remain separate.
Memory is the largest sampled sum of resident memory across the worker and its children.
This measure, RSS, counts memory present in RAM. Sampling can miss short peaks, and shared pages can count twice.
Saved bytes include the database or record files and the task receipt, including the earlier answer retained after editing.
Filesystem block allocation and directory entries add unmeasured overhead.
The database uses more stored bytes in these trials because it also keeps pages and an index.
The index maps each content fingerprint to its stored piece.
The memory samples include the database library. These results establish no memory saving.
One KiB is 1,024 bytes. One MiB is 1,048,576 bytes.

| Project / SDK | Unchanged reuse seconds | Request CPU seconds | Maximum memory MiB | Final saved KiB |
|---|---:|---:|---:|---:|
| boltons / previous | 0.362 | 3.236 | 80.3 | 100.6 |
| boltons / current | 0.366 | 3.030 | 84.3 | 200.1 |
| glob / previous | 0.514 | 1.982 | 53.1 | 313.5 |
| glob / current | 0.345 | 1.392 | 53.0 | 528.1 |
| jmespath / previous | 0.592 | 3.073 | 58.5 | 548.3 |
| jmespath / current | 0.416 | 2.005 | 58.9 | 876.1 |

## What the checks establish

Separate profiles locate storage work. Their timings include profiler overhead and do not enter these tables.
Checks cover reopening, concurrent writers, conflicting bytes, corrupted records and a writer killed before commit.
Python considers `True == 1` true. Both stores compare the saved JSON bytes, so these values cannot overwrite one another under the same fingerprint.
Each SDK audits its own retained state in a separate process beside its declared source.
The application has one current local format and no older-format reader or fallback.
Record contents, fingerprints, verification and request resource limits stay the same.
HTTP stores publish pieces in order; the remote service owns transaction support and durability.

Both approaches use Python. This experiment does not isolate Python overhead against direct command-line use.
Three repetitions and uncontrolled operating-system file caches limit the precision of timing estimates.
These scripted requests establish no general agent, model-token or billing savings.
All projects are familiar. The original first JMESPath result remains unchanged and separately audited.
The earlier hashing comparison remains unchanged and is audited beside the code merged in PR #554.

[Measurements and raw archives](../tests/agent-eval/batched-answer-storage-release/record.json) | [Authenticated profiles and workflow steps](../tests/agent-eval/batched-answer-storage-release/publication.json) | [First storage comparison](batched-answer-storage-first-results.md) | [Earlier hashing comparison](saved-answer-improvement.md)
