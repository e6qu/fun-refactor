# Which tasks are ready for an agent comparison?

Before comparing tools on a bug fix, check that the task and its tests can distinguish a correct repair from a plausible mistake.
For example, a test that accepts an empty implementation cannot grade the requested feature. This page records which candidate tasks passed that review and what still blocks others.

A candidate is a proposed task for an agent comparison. Its grader is the independent program checking submitted repairs.
Admission means the task passed the stated review requirements; it does not authorize a new paid run.
For example, a missing-variable fix needs separate tests for absent, empty and present values.
A reviewer finding no contradiction in one case leaves the other cases unchecked.

## Review decisions and remaining work

Packaging was admitted for one bounded change pilot. That pilot is now permanently stopped
after its first CPU-limit failure. Dotenv and platformdirs still have review gaps and are not
admitted for a new independent comparison by the evidence below.

The task author selected these projects, requirements, reference repairs and graders. Scoped
review and passing finite checks do not establish independent task selection or exhaustive
correctness. Failed calls supply no accepted review, even when readable text remains.
The [candidate pack](../tests/agent-eval/opencode/candidates/README.md) owns source and grader identities.

Illustration: “accept an alternative value only when the environment variable is absent” requires
separate absent, empty and present-variable tests. A repair that treats empty as absent can look
plausible while violating the request. Review those cases before using the task to compare tools.

## Packaging

The proposed task concerns Python version constraints: for example, whether a collection of requirements accepts a candidate package version.
A final version is a normal release; prerelease and postrelease labels carry additional ordering rules.
An intersection asks which versions satisfy several constraints together.
The rows record reviews of that behavior and its API details, not permission to restart the stopped trial.

| Requirement family | Evidence | Remaining limit |
| --- | --- | --- |
| Intersection fallback, matching finals, bounds and exclusions | Both scoped model reviews completed; hosted behavior cases pass | Finite cases; the assessment rejects and preserves the repeated incorrect postrelease finding |
| Constructor/call policies, inference and empty sets | One model review completed; independent hosted checks passed 324 policy combinations | GLM's policy review exhausted output and remains failed |
| Object identity, order, duplicates and one-shot iterables | Both scoped reviews completed; GitHub verified a distinct-object counterexample and the strengthened identity check | No exhaustive iterator or object-behavior proof |
| Existing `contains` and individual `Specifier` APIs | Both scoped reviews completed; existing API cases and unchanged-source checks pass | Only the supported source and declared cases are covered |

The [scoped review and assessment](../tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped/README.md)
records all eleven areas. Its [admission decision](../tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped/admission.json)
keeps the missing independent policy review explicit. Earlier
[distinct-object evidence](../tests/agent-eval/opencode/reviews/2026-10-06-configured/README.md)
preserves the grader counterexample and original memory failure.

The [change pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md)
retains one failed capture and three unstarted cells. Admission did not guarantee completion.
There is no submitted repair or matched efficiency comparison. Do not resume that collection.

## Other candidates

Dotenv loads environment settings from files. Its task must distinguish a variable that is absent from one present with an empty value.
Platformdirs chooses operating-system paths for application files. Its task must distinguish an absolute path from a relative one before creating directories.
These concrete differences explain why a small passing example cannot close the broader review gaps below.
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

Follow the [current work sequence](../PLAN.md#next-work). First check the worker resource limits
with representative scripted checks. A new collection must freeze its task, grader, tool surface,
models, source, budgets and stop rule before calls. Keep ordinary tools available and report actual
fr use separately. Preserve every historical failure and stopped cell.
