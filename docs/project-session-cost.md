# What does using Python and saving answers cost?

Suppose you ask where `glob_with` is declared and what code a change might affect. You ask again without editing, then change its name in one source file and ask about the old name. The last question checks whether the saved answer expires. It does not deliver a complete rename.

We repeat that session on glob (Rust file matching), Boltons (Python utilities) and JMESPath (Python JSON queries). Each of five approaches gets three sessions per project and must return the same complete answers. All 135 answers agree, including missing names and answers limited by the declared search bounds.

## What we compare

- **CLI:** a small Python script runs the compiled `fr` command and reads its JSON output. It imports no fr Python library.
- **Python, restarted:** start the installed fr Python library for every question, then exit.
- **Python, kept running:** keep that library in one Python process for all three questions.
- **Saved, restarted:** restart Python for each question, save the answer and its dependencies, and validate them before reuse.
- **Saved, kept running:** do the same saved-answer work in one Python process.

The Python library is also called the SDK. A dependency is an input an answer relies on, such as a source file or import search. Validation checks whether those inputs still match. A kept-running Python process still starts the compiled `fr` command for each native request. This experiment adds no daemon or new user interface.

All approaches start with separate empty fr caches and keep those caches across their three questions. A cache stores analysis so later requests can avoid repeating work. Only the saved approaches also retain answers and task plans. Order rotates by project and repetition; operating-system file caches remain uncontrolled.

## Complete session time

Seconds below include the coordinator, source copy, Python starts, questions, edit, validation, evidence recording, explicit shutdown and cleanup. Each value is the median: the middle of three measurements. Percentages compare that median with the CLI median in the same row.

| Project | CLI seconds | Python restarted | Python kept running | Saved restarted | Saved kept running |
|---|---:|---:|---:|---:|---:|
| glob | 0.385 | 0.662 (+72.0%) | 0.485 (+25.9%) | 0.790 (+105.2%) | 0.605 (+57.2%) |
| boltons | 1.377 | 1.643 (+19.3%) | 1.470 (+6.7%) | 1.724 (+25.2%) | 1.560 (+13.2%) |
| jmespath | 0.582 | 0.871 (+49.6%) | 0.706 (+21.2%) | 1.076 (+84.9%) | 0.901 (+54.7%) |

The CLI baseline includes a Python coordinator. These percentages measure the added library, process and saved-answer work in this procedure; they do not compare Python with Rust as programming languages. All approaches use the same optimized native executable.

## Processor work, memory and stored data

CPU seconds sum user and system processor work for the session and its waited-for children, from GNU time, rounded to 0.01 seconds. Elapsed seconds also include waiting. The separate process monitor uses a coarse CPU clock that can report zero for short processes; those samples remain in raw diagnostics and do not supply this table. Memory is the largest sampled sum of resident memory, meaning memory present in RAM, for that group of processes. Sampling can miss short peaks; shared pages can count twice. One MiB is 1,048,576 bytes and one KiB is 1,024 bytes.

| Project / approach | Median CPU seconds | Maximum sampled memory MiB | Median final fr cache KiB | Median final saved data KiB |
|---|---:|---:|---:|---:|
| glob / cli-fresh | 0.37 | 53.8 | 153.7 | 0.0 |
| glob / sdk-fresh | 0.62 | 70.3 | 153.7 | 0.0 |
| glob / sdk-running | 0.46 | 73.9 | 153.7 | 0.0 |
| glob / saved-fresh | 0.75 | 74.7 | 153.7 | 520.1 |
| glob / saved-running | 0.58 | 78.9 | 153.7 | 520.1 |
| boltons / cli-fresh | 1.36 | 79.7 | 908.9 | 0.0 |
| boltons / sdk-fresh | 1.61 | 94.5 | 908.9 | 0.0 |
| boltons / sdk-running | 1.46 | 100.7 | 908.9 | 0.0 |
| boltons / saved-fresh | 1.70 | 98.6 | 908.9 | 204.1 |
| boltons / saved-running | 1.53 | 97.1 | 908.9 | 204.1 |
| jmespath / cli-fresh | 0.57 | 55.3 | 297.9 | 0.0 |
| jmespath / sdk-fresh | 0.85 | 76.4 | 297.9 | 0.0 |
| jmespath / sdk-running | 0.69 | 78.8 | 297.9 | 0.0 |
| jmespath / saved-fresh | 1.06 | 80.0 | 297.9 | 872.1 |
| jmespath / saved-running | 0.86 | 83.8 | 297.9 | 872.1 |

Full answers remain local; clients reply with content fingerprints, which identify the answer bytes. Equal evidence objects share one recorded file. Evidence hashing, database snapshots and recording are instrumentation included in elapsed time. Evidence archives are separate from the production caches and saved data above. Directory and filesystem block overhead are unmeasured.

## Setup and limits

The installed SDK has precompiled modules checked against its exact source. These one-time setup costs are outside session totals:

| Setup | Seconds |
|---|---:|
| boltons download seconds | 0.386 |
| bytecode compile seconds | 0.102 |
| glob download seconds | 0.243 |
| jmespath download seconds | 0.270 |
| wheel build seconds | 0.715 |
| wheel install seconds | 0.264 |

Native compilation, dependency provisioning and runner startup are workflow setup; inspect their durations in the linked run. Source archives, their licenses, exact versions, invocation arguments, individual timings and limits are retained in the raw record. Each session remains bounded by 20 elapsed seconds, 10 sampled CPU seconds, 512 MiB sampled memory and 16 MiB workspace growth. These trusted-runner process limits are sampled controls, not a security sandbox.

All three projects were already familiar. The original first JMESPath evaluation remains unchanged and separately audited. Three repetitions cannot establish general savings. There is no live agent, model-token, billing or CodeGraph comparison here. Earlier measurements use different procedures and runners; compare approaches within this run.

[Raw record](../tests/agent-eval/results/2026-10-22-project-sessions-40388759545/result.json) | [Workflow run](https://github.com/e6qu/fun-refactor/actions/runs/40388759545) | [Previous saved-answer comparison](distinct-answer-hashing.md)
