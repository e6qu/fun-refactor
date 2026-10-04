# Review proposed repairs against their requirements

This collection asks whether the task author's proposed reference repairs violate specific
requirements. The source under `src/` includes the reference edits. Original pinned source is
available under `baseline/`; `review/reference.json` records the exact edits. A reference is a
proposal to inspect, not an accepted answer. The graders are also provided.

The three questions cover dotenv environment precedence, packaging's explicit prerelease policies,
and preservation of absolute XDG site paths. Earlier collections asked about other properties while
withholding reference repairs. Their stopped cells remain stopped; none is retried here.

`plan.json`, `inputs.json.gz` and this collector are committed before calls. The frozen plan allocates
Kimi K3 and GLM 5.3 Flash once per question, in that order. Each call keeps the existing 120-second
wall limit, 20 sampled CPU seconds, 768 MiB RSS and 16 MiB disk limit. Two consecutive failures stop
the collection. No retries or increased limits are allowed. The workstation guard also applies.

| Question | Packet bytes | Source bytes |
| --- | ---: | ---: |
| dotenv reference precedence | 6,221 | 4,794 |
| packaging reference policies | 5,523 | 4,123 |
| platformdirs reference paths | 7,923 | 6,095 |

Preparation uses fr to select declaration bytes and compares them with the exact repaired files.
Docstrings are omitted from initial excerpts; other frozen source remains available through read-only
tools. Calls cannot edit or execute candidate code. Source citations alone do not verify findings.
Any proposed counterexample needs separate behavior checking on GitHub before changing a grader.

Protocol 2 explicitly records reference disclosure in both the plan and report. Protocol 1 plans and
reports retain their original meaning and replay unchanged. Seeing a proposed solution changes the
review conditions; do not pool these calls with reviews that withheld it or with code-change trials.
Narrow reviews do not establish full task acceptance, independent task selection or an efficiency gain.

No calls have run at this freeze. The final report will retain every allocated cell, including failures
and stopped cells. Actual billing and complete provider context remain unknown.
