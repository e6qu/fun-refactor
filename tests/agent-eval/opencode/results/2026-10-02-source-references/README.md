# Cite retrieved code without copying it

This frozen development pilot reuses one reviewed cachetools task, two configured models and
three tool arms. It checks source-reference transport. It is not an independent comparison.
The new task permits source IDs; the factual rubric and required source anchors are unchanged.
The plan retains the original source-import provenance text. This task has already appeared in
previous trials and has no held-out status.

Implementation commit: `0d10081ccd34c48487e0f745f38a1e122d840120`. Plan: `d62ebf6016f7ffa5e18e703ad8209aa37bc988e02f5311ef283f856136bcd5b6`.

The tools were OpenCode 1.18.34 and the published fr 0.46.0 binary. No local build ran.
All six cells had one attempt, in frozen order, with the original limits.
Each attempt had a 120-second wall limit, 24 calls and 12 steps. Each monitored process group
had a 20-second CPU cap and 768 MiB sampled RSS cap. Attempt storage growth had a 16 MiB cap.
The outer workstation guard also enforced serial execution, half-core CPU throttling,
1 GiB sampled aggregate RSS, the 64 GiB disk reserve and a 180-second workload deadline.
Peak observed process-group RSS was 668.3 MiB. Retained evidence occupies about 1.1 MiB.

## Outcomes

| Model | Ordinary tools | fr available | fr with public guidance |
|---|---|---|---|
| Kimi K3 | 120-second timeout | Pass, 72.9 s; no fr calls | Pass, 86.9 s; three fr calls |
| GLM 5.3 Flash | 120-second timeout | 120-second timeout | 120-second timeout |

Both completed attempts passed all nine factual/source checks and submitted five reference
citations. The guided Kimi attempt used names, behavior, then a source continuation at byte 2048.
Its ordinary reads also accepted nonzero offsets with empty hashes. The other passing attempt
used ordinary tools throughout, despite having fr available.

| Completed Kimi attempt | Tool calls | Source bytes available | Repeated source bytes | Produced tool-result bytes | Reference metadata bytes | Submitted answer bytes |
|---|---:|---:|---:|---:|---:|---:|
| fr available | 8 | 9,119 | 2,059 | 14,644 | 2,070 | 650 |
| fr with guidance | 9 | 6,784 | 56 | 19,736 | 1,793 | 2,294 |

The guided run delivered less source but produced more total tool output and a larger answer.
A smaller source counter alone does not establish lower context cost. These single observations
on a reviewed task do not establish a benefit from fr, guidance or references.

All four incomplete attempts reached the unchanged deadline. Their retained host logs contain
no submission or tool errors. The ordinary-tools Kimi attempt retained only list_files.
No session export exists for these failures, so their complete source, token and context totals
remain unknown. Partial logs are not complete usage measurements.

`report.json` replays the original outcomes; `summary.json` separates complete audited metrics
from retained partial tool activity. Exact runner files, prompts, schemas, streams and artifact
hashes remain available. The finalizer removed disposable source workspaces after replay.

All 43 frozen source files and executable bits match the public
[cachetools revision](https://github.com/tkem/cachetools/tree/3c082c654c2804b9354e4b62dbd2994f1aac464d).
An anonymous upstream download confirmed the content match before the first model call;
archive packaging hashes differ. Provider credentials stayed in the existing OpenCode config.

The next work is independently graded code changes, including total tool/context costs.
This pilot changes no technical acceptance count: seven of eighteen items demonstrated,
zero of four milestones complete.
