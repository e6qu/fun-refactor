# Native code changes with separate behavior grading

Two reviewed tasks, two configured models, and ordinary/fr discovery arms.
`itsdangerous` appeared in earlier explanations; `blinker` is new to these trials.
These are integration cases, not an independently reviewed efficiency study.

Implementation commit: `d720fdc0bd5080a032035ef2d7be201cfe10f6fc`. Plan: `76243d6223c4b764fb85a16d2437715b2b611a45e211f350af5562f9046c6217`.

The eight cells run once in frozen order. Keep all failures and original records.
Collection is local under fr-local-guard; candidate code never executes locally.
Each attempt retains the existing 120-second wall, 20-second sampled CPU, 768 MiB RSS,
16 MiB growth, 24-call and 12-step ceilings. Complete context/billing remains unknown.

Behavior grading runs only on GitHub using the frozen image, cases and runner.
A submitted patch is not a passing task. Seven submissions replay successfully; one GLM attempt timed out.
No attempt called fr, including the retained partial host log. Behavior grading is separate and pending.

| Task | Model | Arm | Collection | Seconds | Calls | Edits |
|---|---|---|---|---:|---:|---:|
| empty-key-collection | GLM 5.3 Flash | fr | failed | 120.1 | unknown | unknown |
| empty-key-collection | GLM 5.3 Flash | files | submitted | 91.5 | 11 | 1 |
| empty-key-collection | Kimi K3 | files | submitted | 64.4 | 4 | 1 |
| empty-key-collection | Kimi K3 | fr | submitted | 53.0 | 5 | 1 |
| signal-name-type | Kimi K3 | files | submitted | 34.5 | 6 | 1 |
| signal-name-type | Kimi K3 | fr | submitted | 91.8 | 10 | 3 |
| signal-name-type | GLM 5.3 Flash | files | submitted | 105.5 | 11 | 2 |
| signal-name-type | GLM 5.3 Flash | fr | submitted | 107.3 | 13 | 2 |

All attempts used OpenCode 1.18.34 and published fr 0.46.0. Maximum sampled process-group RSS was 684.6 MiB.
The workstation guard also bounded aggregate local work. No retry or limit increase occurred.

The fr-available arm adds 635 tool-schema bytes; none of these attempts used its discovery tool.
Differences between these single attempts do not measure fr execution or establish an efficiency effect.
Failed-attempt usage and complete provider context remain unknown. See `collection-report.json`
for verified transcript metrics and `collection-summary.json` for separately labeled partial host activity.
