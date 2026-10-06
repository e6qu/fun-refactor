# What has actually been reviewed in the three candidate tasks?

The candidates are not yet accepted for an independent ordinary-tools/fr comparison.
The task author chose the projects, requirements, reference repairs and graders. Passing those
graders checks consistency; it does not supply independent task selection or complete review.

This table separates completed, limited review from gaps. A no-finding answer is not a correctness
proof. Failed or stopped calls supply no accepted review, even when their transcripts contain useful text.
The [candidate pack](../tests/agent-eval/opencode/candidates/README.md) owns source and grader identities.

| Requirement family | Completed independent review | Remaining gap |
|---|---|---|
| dotenv flat, nonrecursive alternate words | Kimi identified recursive expansion accepted by the old grader; GitHub verified the counterexample and strengthened check | Broader malformed and adjacent-expression behavior |
| dotenv environment/file precedence | Kimi found no scoped contradiction in the disclosed reference review | Other reviewers and behavior outside that question |
| dotenv absent, empty and present variables | No accepted review from the single absent-variable collection; Kimi submitted repeatedly and the final GLM cell stopped | All three states through both public APIs; do not treat readable failed answers as completion |
| dotenv existing interpolation and disabled interpolation | The broad boundary calls timed out | Existing syntax, unassigned keys and `interpolate=False` |
| packaging exact `Version` object identity | Kimi and GLM found no contradiction for two in-range prerelease objects, in order | Other object inputs, duplicate items, one-shot iterable behavior and empty sets |
| packaging intersection and explicit prerelease policies | The earlier Kimi finding about `>1.0` and `1.0.post1` was rejected against source, specification and execution | Valid review of fallback, matching finals, constructor/call overrides, bounds and exclusions |
| packaging unchanged comparison APIs | No completed review establishes preservation | `contains` and individual `Specifier` behavior |
| platformdirs absolute site-path lists | Kimi found no scoped contradiction in the disclosed reference review | Invalid-entry/default handling and remaining site-path API behavior |
| platformdirs rejected relative directory creation | Kimi found no contradiction for one `XDG_CONFIG_HOME` value with `ensure_exists=True` | Other user-home variables, site lists and pre-existing filesystem states; GLM failed to submit |
| platformdirs remaining compatibility | No completed review of the broad user-API question | Defaults, suffixes, wrappers, iterators, runtime/media preservation and non-Unix scope |

Evidence: [flat-grammar counterexample](../tests/agent-eval/opencode/reviews/2026-10-04-packets/README.md),
[reference reviews and rejected finding](../tests/agent-eval/opencode/reviews/2026-10-05-references/README.md),
[stopped boundary collection](../tests/agent-eval/opencode/reviews/2026-10-05-boundaries/README.md),
and [single-assertion reviews](../tests/agent-eval/opencode/reviews/2026-10-05-assertions/README.md).

## What the next work must resolve

The single-assertion collection completed three reviews covering two assertions, then stopped after
two submission failures. It must stay stopped. Scripted OpenCode checks now verify terminal answers,
including missing and duplicate submissions and interrupted work. Live model reliability remains open.
Do not silently accept text answers, discard extra calls, raise limits or reinterpret old failures.
Use scripted protocol checks first; any later live collection needs a new frozen design and scope.

The [prepared terminal design](../tests/agent-eval/opencode/reviews/2026-10-06-terminal-design/README.md)
asks three new scoped questions about disabled interpolation, a one-shot iterable with duplicates,
and invalid site-path fallback. Source and packet checks pass, but no model calls have been made.
It needs a frozen executable plan and separate hosted provider credentials. None of the gaps in
the table closes merely because a question is ready.

Finish the missing task-clarity, baseline, alternative-repair and grader-blind-spot review before
freezing independent change trials. A reviewed assertion can be recorded without accepting its whole
task. Keep ordinary tools available and measure actual fr adoption separately from tool availability.

The current road map remains **7 of 18 technical items demonstrated, with all four milestones open**.
Delegation, source-connected proof tasks, complete provider cost accounting and a matched unfamiliar-task
efficiency comparison remain outside the evidence above.
