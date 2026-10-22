# Hash each distinct answer piece once

This is the first measurement, before the memory-lifetime fix. Its timing and storage results remain unchanged.
Later regression checks found that temporary data could survive completed calls until Python ran cyclic garbage collection.
Use the [current guide](distinct-answer-hashing.md) for the corrected behavior and new measurements.

A user asks where `glob_with` appears and which code a change could affect.
The Python helper library, called the SDK, can save that answer and the inputs it depends on.
A content fingerprint is a label computed from a saved piece's contents. Computing that label is called hashing.
JSON is the text format used to save those pieces, including numbers, lists and named fields.
For example, the same function name can appear in many parts of an answer.
Previously, packing the glob answer visited 11,210 pieces and hashed every visit, although only 1,387 pieces were distinct.
The SDK now remembers fingerprints within each operation. Equal pieces share a fingerprint without repeating that hash.
This includes separately created lists and objects with equal contents, not just repeated references to one Python object.
The SDK still visits every input entry. It skips the repeated fingerprint computation.
The lookup was intended to end with the operation, but could remain in memory until cyclic garbage collection ran.
Editing an input before the next call still produced a new fingerprint.
Stored pieces, their fingerprints and corruption checks remain identical.

## What is compared

The previous SDK is the code merged in PR #555, which already writes an answer's pieces together into one database.
Both SDKs call the same compiled `fr` program, built with optimizations, on the same runner.
Both ask identical questions on fixed versions of glob, Boltons and JMESPath at the same absolute project path.
glob matches file paths in Rust. Boltons provides Python utilities. JMESPath selects values from JSON documents.
Each project and SDK has three sessions containing first use, unchanged reuse and one source edit.
Each question starts a new Python process.
Both approaches let the compiled `fr` program keep its own saved analysis between questions, starting empty.
The saved approach also keeps its answer and a task plan, which records the inputs needed for the answer to remain valid.
The fresh approach asks `fr` again without saving a task plan or answer.
After an edit, the saved approach checks those inputs, expires the old answer and repeats discovery.
The edit uses `fr file replace` in one file. For example, glob replaces `glob_with` with `glob_with_cost_renamed`.
It then asks about the old name. This checks whether an answer expires; it does not deliver a complete project rename.
SDK order alternates by project. Saved and fresh order alternates by repetition within each SDK.
Each SDK finishes its three repetitions on a project before the other SDK starts that project.
All 108 request answers agree across SDKs, including missing names, ambiguous matches and answers that stop at their work limit.

## Complete session time

Each table uses three trials. The median is the middle value after sorting those three measurements.
Median seconds include Python startup, validation, discovery, saving, output, source copies, the edit and cleanup.
Builds, dependency installation and initial downloads remain separate in raw reports and workflow steps.

| Project | Previous saved | Current saved | Change | Previous fresh | Current fresh | Current saved overhead versus fresh |
|---|---:|---:|---:|---:|---:|---:|
| boltons | 3.309 | 3.262 | -1.4% | 3.108 | 3.080 | +5.9% |
| glob | 1.595 | 1.501 | -5.9% | 1.193 | 1.202 | +24.9% |
| jmespath | 2.185 | 2.029 | -7.1% | 1.616 | 1.588 | +27.8% |

Negative change means less time than the previous saved approach.
Positive overhead means saving costs more than asking again with the current SDK.
Percentages compare the median complete session times above.

## CPU, memory and saved bytes

CPU time measures processor work in Python and any `fr` programs it starts. Elapsed time also includes waiting.
Request CPU sums the three questions, sampled before the final report and answer writes.
The GNU `time` command also measures those writes and rounds processor time to 0.01 seconds.
Processor work for copying, editing and cleanup remains separate.
Memory is the largest sampled sum of resident memory across Python and the programs it starts.
This measure, RSS, counts memory present in RAM. Sampling can miss short peaks, and shared pages can count twice.
Saved bytes include the database and saved task record, including the earlier answer retained after editing.
Filesystem block allocation and directory entries add unmeasured overhead.
Both SDKs use the same database format. This change is intended to reduce repeated computation, not stored bytes.
The temporary lookup adds one entry per distinct piece while hashing. The memory samples include that working space.
The memory samples include the database library. These results establish no memory saving.
One KiB is 1,024 bytes. One MiB is 1,048,576 bytes.

| Project / SDK | Unchanged reuse seconds | Request CPU seconds | Maximum memory MiB | Final saved KiB |
|---|---:|---:|---:|---:|
| boltons / previous | 0.333 | 3.140 | 81.9 | 200.1 |
| boltons / current | 0.326 | 3.102 | 81.6 | 200.1 |
| glob / previous | 0.337 | 1.423 | 48.9 | 516.1 |
| glob / current | 0.337 | 1.329 | 51.1 | 516.1 |
| jmespath / previous | 0.391 | 2.023 | 58.1 | 860.1 |
| jmespath / current | 0.390 | 1.853 | 58.0 | 860.1 |

## What the checks establish

Separate profiles locate hashing work. Their timings include profiler overhead and do not enter these tables.
Tests compare fingerprints with an independent implementation and the compiled `fr` program.
Tests also check one fingerprint computation per distinct piece and changed fingerprints after later edits.
A list containing itself and numbers such as infinity cannot form valid JSON inputs; both fail visibly.
Integers, decimal numbers, booleans and signed zero remain distinct.
For example, `1`, `1.0` and `true` cannot share a stored piece. The fingerprints for `0.0` and `-0.0` also differ.
Checks cover reopening, concurrent writers, conflicting bytes, corrupted records and a writer killed before commit.
Python considers `True == 1` true. Both stores compare the saved JSON bytes, so these values cannot overwrite one another under the same fingerprint.
Each SDK audits its own retained state in a separate process beside its declared source.
The application has one current local format and no older-format reader or fallback.
Record contents, fingerprints, verification and request resource limits stay the same.

Both approaches use Python. This experiment does not isolate Python overhead against direct command-line use.
Three repetitions and uncontrolled operating-system file caches limit the precision of timing estimates.
These scripted requests establish no general agent, model-token or billing savings.
All projects are familiar. The original first JMESPath result remains unchanged and separately audited.
The earlier storage and hashing comparisons remain unchanged and are audited beside their measured code.

[Measurements and raw archives](../tests/agent-eval/distinct-answer-hashing/record.json) | [Authenticated profiles and workflow steps](../tests/agent-eval/distinct-answer-hashing/publication.json) | [Previous storage comparison](batched-answer-storage.md) | [Earlier hashing comparison](saved-answer-improvement.md)
