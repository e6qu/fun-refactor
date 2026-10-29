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
| glob | 0.812 | 1.037 (+27.6%) | 0.819 (+0.8%) | 1.215 (+49.5%) | 0.966 (+18.9%) |
| boltons | 2.362 | 2.717 (+15.0%) | 2.497 (+5.7%) | 2.845 (+20.4%) | 2.820 (+19.4%) |
| jmespath | 0.819 | 1.188 (+45.0%) | 0.973 (+18.7%) | 1.460 (+78.3%) | 1.225 (+49.5%) |

The CLI baseline includes a Python coordinator. These percentages measure the added library, process and saved-answer work in this procedure; they do not compare Python with Rust as programming languages. All approaches use the same optimized native executable.

## Processor work, memory and stored data

CPU seconds sum user and system processor work for the session and its waited-for children, from GNU time, rounded to 0.01 seconds. Elapsed seconds also include waiting. The separate process monitor uses a coarse CPU clock that can report zero for short processes; those samples remain in raw diagnostics and do not supply this table. Memory is the largest sampled sum of resident memory, meaning memory present in RAM, for that group of processes. Sampling can miss short peaks; shared pages can count twice. One MiB is 1,048,576 bytes and one KiB is 1,024 bytes.

| Project / approach | Median CPU seconds | Maximum sampled memory MiB | Median final fr cache KiB | Median final saved data KiB |
|---|---:|---:|---:|---:|
| glob / cli-fresh | 0.67 | 51.7 | 153.7 | 0.0 |
| glob / sdk-fresh | 1.00 | 79.0 | 153.7 | 0.0 |
| glob / sdk-running | 0.80 | 79.3 | 153.7 | 0.0 |
| glob / saved-fresh | 1.18 | 80.4 | 153.7 | 512.1 |
| glob / saved-running | 0.95 | 82.5 | 153.7 | 512.1 |
| boltons / cli-fresh | 2.35 | 78.7 | 908.9 | 0.0 |
| boltons / sdk-fresh | 2.69 | 108.0 | 908.9 | 0.0 |
| boltons / sdk-running | 2.48 | 103.4 | 908.9 | 0.0 |
| boltons / saved-fresh | 2.79 | 105.3 | 908.9 | 192.1 |
| boltons / saved-running | 2.54 | 105.5 | 908.9 | 192.1 |
| jmespath / cli-fresh | 0.81 | 58.8 | 297.9 | 0.0 |
| jmespath / sdk-fresh | 1.15 | 83.6 | 297.9 | 0.0 |
| jmespath / sdk-running | 0.96 | 84.6 | 297.9 | 0.0 |
| jmespath / saved-fresh | 1.44 | 85.5 | 297.9 | 868.1 |
| jmespath / saved-running | 1.20 | 88.8 | 297.9 | 868.1 |

Full answers remain local; clients reply with content fingerprints, which identify the answer bytes. Equal evidence objects share one recorded file. Evidence hashing, database snapshots and recording are instrumentation included in elapsed time. Evidence archives are separate from the production caches and saved data above. Directory and filesystem block overhead are unmeasured.

## Setup and limits

The installed SDK has precompiled modules checked against its exact source. These one-time setup costs are outside session totals:

| Setup | Seconds |
|---|---:|
| boltons download seconds | 0.138 |
| bytecode compile seconds | 0.140 |
| glob download seconds | 0.089 |
| jmespath download seconds | 0.090 |
| wheel build seconds | 0.715 |
| wheel install seconds | 0.364 |

Native compilation, dependency provisioning and runner startup are workflow setup; inspect their durations in the linked run. Source archives, their licenses, exact versions, invocation arguments, individual timings and limits are retained in the raw record. Each session remains bounded by 20 elapsed seconds, 10 sampled CPU seconds, 512 MiB sampled memory and 16 MiB workspace growth. These trusted-runner process limits are sampled controls, not a security sandbox.

All three projects were already familiar. The original first JMESPath evaluation remains unchanged and separately audited. Three repetitions cannot establish general savings. There is no live agent, model-token, billing or CodeGraph comparison here. Earlier measurements use different procedures and runners; compare approaches within this run.

[Raw record](../tests/agent-eval/results/2026-10-22-project-sessions-40890645244/result.json) | [Workflow run](https://github.com/e6qu/fun-refactor/actions/runs/40890645244) | [Previous saved-answer comparison](distinct-answer-hashing.md)
