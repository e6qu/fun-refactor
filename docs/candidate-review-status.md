# Candidate task review and admission

Packaging was admitted for one bounded change pilot. That pilot is now permanently stopped
after its first CPU-limit failure. Dotenv and platformdirs still have review gaps and are not
admitted for a new independent comparison by the evidence below.

The task author selected these projects, requirements, reference repairs and graders. Scoped
review and passing finite checks do not establish independent task selection or exhaustive
correctness. Failed calls supply no accepted review, even when readable text remains.
The [candidate pack](../tests/agent-eval/opencode/candidates/README.md) owns source and grader identities.

## Packaging

| Requirement family | Evidence | Remaining limit |
| --- | --- | --- |
| Intersection fallback, matching finals, bounds and exclusions | Both scoped model reviews completed; hosted behavior cases pass | Finite cases; the assessment rejects and preserves the repeated incorrect postrelease finding |
| Constructor/call policies, inference and empty sets | One model review completed; independent hosted checks passed 324 policy combinations | GLM's policy review exhausted output and remains failed |
| Object identity, order, duplicates and one-shot iterables | Both scoped reviews completed; GitHub verified a distinct-object counterexample and the strengthened identity check | No exhaustive iterator or object-behavior proof |
| Existing `contains` and individual `Specifier` APIs | Both scoped reviews completed; existing API cases and unchanged-source checks pass | Only the admitted source and declared cases are covered |

The [scoped review and assessment](../tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped/README.md)
records all eleven areas. Its [admission decision](../tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped/admission.json)
keeps the missing independent policy review explicit. Earlier
[distinct-object evidence](../tests/agent-eval/opencode/reviews/2026-10-06-configured/README.md)
preserves the grader counterexample and original memory failure.

The [change pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md)
retains one failed capture and three unstarted cells. Admission did not guarantee completion.
There is no submitted repair or matched efficiency comparison. Do not resume that collection.

## Other candidates

| Requirement family | Completed review | Remaining gap |
| --- | --- | --- |
| Dotenv flat alternate words | Kimi found recursive expansion accepted by the old grader; GitHub verified and strengthened the check | Malformed and adjacent-expression behavior |
| Dotenv environment/file precedence | Kimi found no scoped contradiction | Other reviewers and behavior outside the question |
| Dotenv absent, empty and present variables | No accepted absent-variable review; submission failures stopped the collection | All three states through both public APIs |
| Dotenv existing and disabled interpolation | Both models found no contradiction for one literal `interpolate=False` input | Other disabled inputs, existing syntax and unassigned keys |
| Platformdirs absolute site-path lists | Kimi found no scoped contradiction | Invalid entries, defaults and remaining site-path APIs |
| Platformdirs rejected relative directory creation | Kimi reviewed one `XDG_CONFIG_HOME` input with `ensure_exists=True` | Other variables, site lists and filesystem states; GLM failed to submit |
| Platformdirs remaining compatibility | No completed broad user-API review | Defaults, suffixes, wrappers, iterators and preserved platform behavior |

Evidence: [flat-grammar counterexample](../tests/agent-eval/opencode/reviews/2026-10-04-packets/README.md),
[reference reviews](../tests/agent-eval/opencode/reviews/2026-10-05-references/README.md),
[single-assertion reviews](../tests/agent-eval/opencode/reviews/2026-10-05-assertions/README.md), and
[configured reviews](../tests/agent-eval/opencode/reviews/2026-10-06-configured/README.md).

## Before another comparison

Follow the [current work sequence](../PLAN.md#next-large-chunk). First resolve resource admission
with representative scripted checks. A new collection must freeze its task, grader, tool surface,
models, source, budgets and stop rule before calls. Keep ordinary tools available and report actual
fr use separately. Preserve every historical failure and stopped cell.
