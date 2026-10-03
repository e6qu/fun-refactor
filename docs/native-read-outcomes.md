# Native source-reading outcomes and observed costs

Original outcomes are preserved. Tool output includes failed attempts. Collection time omits
grader, provider, host and cache costs. Source-page counts omit code in metadata; total context
and provider billing remain unknown. Tool availability does not establish fr use or benefit.

## 2026-10-02-source-references

| Model | Tools | Outcome | Calls | fr calls | Result bytes | Source bytes | Seconds |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zai-coding-plan/glm-5.3-flash | fr-guided | failed | 9 | 0 | 28468 | 14912 | 120.1 |
| zai-coding-plan/glm-5.3-flash | files | failed | 7 | 0 | 26962 | 15279 | 120.1 |
| zai-coding-plan/glm-5.3-flash | fr | failed | 16 | 0 | 44510 | 22837 | 120.1 |
| kimi-code-plan-global/k3 | fr-guided | passed | 9 | 3 | 19736 | 6784 | 86.9 |
| kimi-code-plan-global/k3 | files | failed | 1 | 0 | 1028 | 0 | 120.0 |
| kimi-code-plan-global/k3 | fr | passed | 8 | 0 | 14644 | 9119 | 72.9 |

Comparable successful ordinary/fr pairs: 0.

The JSON report retains partial step usage, produced versus native-confirmed results,
repeated source, configuration bytes and per-model/arm totals including failures.
Zero successful outcomes leave time per pass undefined. Missing attempts stay missing.
