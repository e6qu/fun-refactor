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
| glob | 0.629 | 1.046 (+66.4%) | 0.795 (+26.4%) | 1.346 (+114.1%) | 1.002 (+59.3%) |
| boltons | 2.372 | 2.807 (+18.3%) | 2.489 (+4.9%) | 2.990 (+26.0%) | 2.637 (+11.1%) |
| jmespath | 0.956 | 1.404 (+47.0%) | 1.142 (+19.5%) | 1.836 (+92.1%) | 1.501 (+57.1%) |

The CLI baseline includes a Python coordinator. These percentages measure the added library, process and saved-answer work in this procedure; they do not compare Python with Rust as programming languages. All approaches use the same optimized native executable.

## Processor work, memory and stored data

CPU seconds sum user and system processor work for the session and its waited-for children, from GNU time, rounded to 0.01 seconds. Elapsed seconds also include waiting. The separate process monitor uses a coarse CPU clock that can report zero for short processes; those samples remain in raw diagnostics and do not supply this table. Memory is the largest sampled sum of resident memory, meaning memory present in RAM, for that group of processes. Sampling can miss short peaks; shared pages can count twice. One MiB is 1,048,576 bytes and one KiB is 1,024 bytes.

| Project / approach | Median CPU seconds | Maximum sampled memory MiB | Median final fr cache KiB | Median final saved data KiB |
|---|---:|---:|---:|---:|
| glob / cli-fresh | 0.61 | 48.1 | 153.7 | 0.0 |
| glob / sdk-fresh | 1.02 | 73.4 | 153.7 | 0.0 |
| glob / sdk-running | 0.78 | 73.4 | 153.7 | 0.0 |
| glob / saved-fresh | 1.28 | 79.0 | 153.7 | 532.1 |
| glob / saved-running | 0.97 | 79.4 | 153.7 | 532.1 |
| boltons / cli-fresh | 2.36 | 74.3 | 908.9 | 0.0 |
| boltons / sdk-fresh | 2.79 | 101.3 | 908.9 | 0.0 |
| boltons / sdk-running | 2.47 | 98.0 | 908.9 | 0.0 |
| boltons / saved-fresh | 2.92 | 108.2 | 908.9 | 200.1 |
| boltons / saved-running | 2.62 | 105.4 | 908.9 | 200.1 |
| jmespath / cli-fresh | 0.94 | 57.6 | 297.9 | 0.0 |
| jmespath / sdk-fresh | 1.38 | 74.0 | 297.9 | 0.0 |
| jmespath / sdk-running | 1.13 | 81.1 | 297.9 | 0.0 |
| jmespath / saved-fresh | 1.77 | 80.2 | 297.9 | 852.1 |
| jmespath / saved-running | 1.47 | 79.8 | 297.9 | 852.1 |

Full answers remain local; clients reply with content fingerprints, which identify the answer bytes. Equal evidence objects share one recorded file. Evidence hashing, database snapshots and recording are instrumentation included in elapsed time. Evidence archives are separate from the production caches and saved data above. Directory and filesystem block overhead are unmeasured.

## Setup and limits

The installed SDK has precompiled modules checked against its exact source. These one-time setup costs are outside session totals:

| Setup | Seconds |
|---|---:|
| boltons download seconds | 0.135 |
| bytecode compile seconds | 0.172 |
| glob download seconds | 0.088 |
| jmespath download seconds | 0.076 |
| wheel build seconds | 1.016 |
| wheel install seconds | 0.415 |

Native compilation, dependency provisioning and runner startup are workflow setup; inspect their durations in the linked run. Source archives, their licenses, exact versions, invocation arguments, individual timings and limits are retained in the raw record. Each session remains bounded by 20 elapsed seconds, 10 sampled CPU seconds, 512 MiB sampled memory and 16 MiB workspace growth. These trusted-runner process limits are sampled controls, not a security sandbox.

All three projects were already familiar. The original first JMESPath evaluation remains unchanged and separately audited. Three repetitions cannot establish general savings. There is no live agent, model-token, billing or CodeGraph comparison here. Earlier measurements use different procedures and runners; compare approaches within this run.

[Raw record](../tests/agent-eval/results/2026-10-22-project-sessions-40236408645/result.json) | [Workflow run](https://github.com/e6qu/fun-refactor/actions/runs/40236408645) | [Previous saved-answer comparison](distinct-answer-hashing.md)
