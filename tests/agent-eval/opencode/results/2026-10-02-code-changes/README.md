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
A submitted patch is not a passing task. Results pending.
