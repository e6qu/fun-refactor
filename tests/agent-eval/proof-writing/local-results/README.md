# Proof-writing comparison with corrected timing

All 36 scheduled attempts are recorded: 25 clients finished and 11 reached the
150-second limit. The clients delivered 25 patches. Independent grading is pending;
finishing a conversation does not establish that its proof is correct.

Each selected agent tried three fixed Rust programs twice in each setup:
ordinary file, search, shell and Lean tools; and those same tools plus fr.
Fr can discover a theorem, check proposed proof steps and save a checked proof.
Each attempt starts a fresh conversation and project.

For example, the page-count request includes proving that 11 items at 5 per page
need 3 pages, and that valid inputs produce a count between zero and the item count.
The agents must keep the program, Lean model and theorem unchanged and edit only
the marked proof region. They receive no reference proof.

The following counts show clients finishing within the deadline, out of two attempts.
They are not independent acceptance counts.

| Agent | Task | Ordinary tools | Ordinary tools plus fr |
| --- | --- | ---: | ---: |
| Luna Fast | Access for staff or the owner | 2/2 | 2/2 |
| Luna Fast | Page counts | 2/2 | 1/2 |
| Luna Fast | A range fits within its limit | 2/2 | 2/2 |
| GLM Flash | Access for staff or the owner | 2/2 | 2/2 |
| GLM Flash | Page counts | 0/2 | 0/2 |
| GLM Flash | A range fits within its limit | 2/2 | 1/2 |
| Kimi K3 | Access for staff or the owner | 2/2 | 2/2 |
| Kimi K3 | Page counts | 0/2 | 0/2 |
| Kimi K3 | A range fits within its limit | 1/2 | 2/2 |

The whole attempt has the same 150-second limit, including preparation, tools,
checks, patch capture and session accounting. CPU, sampled memory and output limits
also stay unchanged. The timer includes system sleep. Idle sleep is prevented during
each attempt; forced sleep would still consume the deadline. No sleep was detected.
Saved timestamps and the elapsed timer differed by at most 0.023 seconds.

Two pauses between attempts allowed review of closed records and serial local checks.
Every closed result stayed unchanged. No failed slot was retried, and the source,
claims, models, selected profiles and limits remained fixed throughout this comparison.
Operating-system and provider caches remain uncontrolled.

M3 remains open: GLM and Kimi delivered no page-count proof with fr in this run.
Earlier failures remain in the [task guide](../README.md). Across all packets,
173 original outcomes are retained.

[Exact settings, timing, usage and reviewed actions](summary.json) |
[Submitted patches](patches/).
Only patches and summaries are public. Full prompts, CLI records, session exports,
working-source archives and account data remain local. Account charges are unknown.
