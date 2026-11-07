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
| glob | 0.561 | 0.847 (+51.0%) | 0.675 (+20.3%) | 1.019 (+81.7%) | 0.809 (+44.2%) |
| boltons | 1.947 | 2.215 (+13.8%) | 2.032 (+4.4%) | 2.313 (+18.8%) | 2.098 (+7.8%) |
| jmespath | 0.695 | 0.991 (+42.5%) | 0.817 (+17.6%) | 1.218 (+75.3%) | 1.025 (+47.5%) |

The CLI baseline includes a Python coordinator. These percentages measure the added library, process and saved-answer work in this procedure; they do not compare Python with Rust as programming languages. All approaches use the same optimized native executable.

## Processor work, memory and stored data

CPU seconds sum user and system processor work for the session and its waited-for children, from GNU time, rounded to 0.01 seconds. Elapsed seconds also include waiting. The separate process monitor uses a coarse CPU clock that can report zero for short processes; those samples remain in raw diagnostics and do not supply this table. Memory is the largest sampled sum of resident memory, meaning memory present in RAM, for that group of processes. Sampling can miss short peaks; shared pages can count twice. One MiB is 1,048,576 bytes and one KiB is 1,024 bytes.

| Project / approach | Median CPU seconds | Maximum sampled memory MiB | Median final fr cache KiB | Median final saved data KiB |
|---|---:|---:|---:|---:|
| glob / cli-fresh | 0.54 | 55.9 | 153.7 | 0.0 |
| glob / sdk-fresh | 0.81 | 75.0 | 153.7 | 0.0 |
| glob / sdk-running | 0.65 | 82.1 | 153.7 | 0.0 |
| glob / saved-fresh | 0.96 | 82.8 | 153.7 | 516.1 |
| glob / saved-running | 0.76 | 81.3 | 153.7 | 516.1 |
| boltons / cli-fresh | 1.92 | 77.1 | 908.9 | 0.0 |
| boltons / sdk-fresh | 2.18 | 109.0 | 908.9 | 0.0 |
| boltons / sdk-running | 2.01 | 109.1 | 908.9 | 0.0 |
| boltons / saved-fresh | 2.25 | 108.0 | 908.9 | 196.1 |
| boltons / saved-running | 2.05 | 108.3 | 908.9 | 196.1 |
| jmespath / cli-fresh | 0.67 | 58.0 | 297.9 | 0.0 |
| jmespath / sdk-fresh | 0.96 | 81.5 | 297.9 | 0.0 |
| jmespath / sdk-running | 0.79 | 87.3 | 297.9 | 0.0 |
| jmespath / saved-fresh | 1.16 | 85.5 | 297.9 | 856.1 |
| jmespath / saved-running | 0.97 | 85.2 | 297.9 | 856.1 |

Full answers remain local; clients reply with content fingerprints, which identify the answer bytes. Equal evidence objects share one recorded file. Evidence hashing, database snapshots and recording are instrumentation included in elapsed time. Evidence archives are separate from the production caches and saved data above. Directory and filesystem block overhead are unmeasured.

## Setup and limits

The installed SDK has precompiled modules checked against its exact source. These one-time setup costs are outside session totals:

| Setup | Seconds |
|---|---:|
| boltons download seconds | 0.307 |
| bytecode compile seconds | 0.109 |
| glob download seconds | 0.186 |
| jmespath download seconds | 0.215 |
| wheel build seconds | 0.615 |
| wheel install seconds | 0.314 |

Native compilation, dependency provisioning and runner startup are workflow setup; inspect their durations in the linked run. Source archives, their licenses, exact versions, invocation arguments, individual timings and limits are retained in the raw record. Each session remains bounded by 20 elapsed seconds, 10 sampled CPU seconds, 512 MiB sampled memory and 16 MiB workspace growth. These trusted-runner process limits are sampled controls, not a security sandbox.

All three projects were already familiar. The original first JMESPath evaluation remains unchanged and separately audited. Three repetitions cannot establish general savings. There is no live agent, model-token, billing or CodeGraph comparison here. Earlier measurements use different procedures and runners; compare approaches within this run.

[Raw record](../tests/agent-eval/results/2026-10-22-project-sessions-42389699341/result.json) | [Workflow run](https://github.com/e6qu/fun-refactor/actions/runs/42389699341) | [Previous saved-answer comparison](distinct-answer-hashing.md)
