# Native source-reading outcomes and observed costs

Use these records to check whether an agent answered the code question correctly and used fr.
For example, finding a source line only helps if the explanation based on it is correct.

The totals include failed attempts. The recorded time does not include every grading or provider cost.
Source-page counts also miss code carried in other report fields. Total reading and billing remain unknown.
Having fr available does not establish that the agent used it or saved work.

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
