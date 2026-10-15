# Choose a way to prepare and deliver an edit

You can ask for a task guide or use individual commands to inspect, preview and apply the same change.
For example, compare those routes for a small helper repair. The measurements below count scripted tool work; they do not tell us which route helps a live agent finish faster.

A route here means a sequence of tool commands, not an HTTP endpoint.
A guide helps choose an operation; an intent describes the requested operation and report sections.
A task change combines known targets, replacement code and checks. Direct authoring previews edits before a separate workflow delivers them.
For a known rounding repair, compare routes using the same replacement and tests.
The byte totals include requests and replies; they do not measure time spent inventing the replacement.

## Measured routes and their limits

This comparison measures the cost of reaching the same checked edit through four public
routes. It runs prescribed programs, without a model. Calls and serialized bytes describe those
programs; they do not establish autonomous agent efficiency or billed token savings.

## What each route contributes

| Route | Use it when | Checks and recovery | Existing consumers |
| --- | --- | --- | --- |
| `guide` → reviewed intent | You know the requested change and need help choosing a supported command | Rechecks the task, selected code, authored edit and required checks before writing | `FrClient.review_guide`, `tests/fixtures/tagged_intent_sdk.py`, `tools/upstream-multibody-agent.py`, `tools/proof-authoring-agent.py` |
| `intent` | You have chosen an operation and need its relevant report sections | Keeps those reports and the complete edit review together for checked execution | `FrClient.compile`, `tests/agent_intent_cli.rs`, capability/recipe/migration/proof clients |
| `task-change` | You already have exact targets, replacement code and named checks | Reviews the complete edit, checks requested states and keeps history and patch delivery | `FrClient.review`, configured change runners, `tools/task-change-context.py` |
| `author batch` → `workflow` | You need to save an edit plan separately before arranging its checks and delivery | Checks the edits first; the later workflow requires the chosen checks for that saved transaction | CLI authoring, `tools/author-batch-context.py`, task template consumers |

These are overlapping layers, not four interchangeable APIs. Guide and intent cover capabilities,
recipes, migrations and proof work beyond structural task changes. A saved direct-author plan is
also useful without delivery. Direct authoring does not attach required checks to the transaction;
its workflow enforces the chosen checks. Task and intent transactions retain that requirement.
No route is scheduled for removal based only on in-repository callers.

Illustration: you have read the rounding function, written its replacement and chosen the regression test.
Use `task-change` to review and deliver those known inputs. If you need help selecting the supported
operation first, use `guide`. The agent still supplies the intended edit and reviews the whole diff.

The instruction entry point is `skills/fr/SKILL.md`. Guide, intents, task and author references
each explain their own manifest and review identity. Task's manual template composition is still
needed before concrete fragments exist. Once inputs are complete, `task-change` removes those
joins. Do not duplicate all four manifests in the entry guide.

## Bounded comparison

Collected on GitHub, with a 15-minute job ceiling and no OpenCode or provider calls:

- Compare a Rust scalar edit, a Python body replacement and two coordinated Rust bodies.
- Use identical starting files, authored changes, required conditions after the edit, named checks, reversal and patch
  delivery in each route. Retain the commands, input payloads, response objects and refusals.
- Check exact final source and patch bytes, all eight apply, check, reversal and delivery stages, and independent behavior.
  Report the direct-author transaction's different check binding rather than hiding it.
- Compare guided execution before and after consolidation using the exact former execution
  function. Keep its source and origin with the report. Count actual calls and canonical JSON, the normalized encoding used to compare records and compute their fingerprints,
  request/response bytes; do not infer time or token savings from those counts.
- Exercise stale source, changed check declarations, changed fragment inputs, a failed original
  check and mutation of a retained guide goal/review. A refused execution must not modify source
  or deliver a patch. Keep any prepared history plan visible in the result.

Guided writes no longer reread the guide in a separate process immediately before intent
execution. Native intent recomputes it in the edit snapshot. SDK review integrity includes the
retained goal, and the comparison exercises native refusals. Read/preview guide actions still
refresh separately because they have no native guide binding.

The former execution function uses the current review objects and native binary. This isolates
the extra guide call; it is not a replay of the entire former SDK. Every arm uses the same
temporary path and a fresh project/history. External fragment bytes are retained separately;
request counts include file-backed manifest contents. Common fixture setup and independent
oracle processes are outside the fr traffic count.

## Results

Read each column as a different command sequence for the same prescribed edit.
The baseline for “Guide after” is “Guide before”; the other route columns are alternatives, not time measurements.
Byte totals include serialized requests and replies, including input-file contents.
They exclude the agent's work to discover the repair, common fixture setup and independent checking processes.
Process counts reveal some extra work, but this report does not measure its execution time or memory overhead.

[Run 37973505951, attempt 2](https://github.com/e6qu/fun-refactor/actions/runs/37973505951)
passed all 38 cells: 15 successful edits and 23 refusals. The
[retained report](../tests/agent-eval/results/2026-10-09-workflow-routes/result.json) contains
every command and response; its manifest records the source commit, binary digest and original
GitHub artifact digest. Seven offline regression tests check completeness and altered records.

Each successful arm produced identical final source and patch bytes and passed all eight
apply, check, reversal and delivery stages plus an independent behavior check. Cells with changed source, checks or
fragment inputs refused. Failed original checks retained an unapplied plan. Changed goal/review
objects refused inside the SDK. None of those 23 cases changed source or delivered a patch.
An author plan already owns its fragment, so changing that external file is not a stale-input
condition for the saved-plan arm and is not counted as a refusal test there.

| Edit | Author + workflow | Task | Intent | Guide after | Guide before |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust scalar | 13,779 | 12,232 | 16,307 | 21,675 | 27,137 |
| Python body | 11,737 | 10,146 | 14,222 | 19,061 | 24,118 |
| Two Rust bodies | 13,090 | 11,760 | 16,671 | 22,160 | 28,109 |
| fr process calls per edit | 6 | 3 | 3 | 3 | 4 |

Cells contain canonical internal request/response bytes. Removing the redundant guide read
saves one process and 5,057–5,949 bytes for these edits. It does not reduce the agent-visible
complete-program bytes: the separate scalar program remains 1,140 bytes including its result.
The [inline task baseline](../tests/agent-eval/agent-guide-context.json) still uses only two
processes and 11,315 internal bytes, versus three and 21,030 for guided delivery in that harness.
The two harnesses count different request envelopes; compare arms within each harness.

Keep all public routes. Use task change when concrete targets, changes and checks are already
known; use the guide when selecting an supported operation. The body-input comparison below removes
repeated caller fields within the guide route.

Two development runs failed: the first expected the wrong scalar formatting; the second compared
an in-memory tuple with a JSON list during auditing. Both remain visible in GitHub
([37972312155](https://github.com/e6qu/fun-refactor/actions/runs/37972312155),
[37973025301](https://github.com/e6qu/fun-refactor/actions/runs/37973025301)). The final run's first
attempt never acquired a runner; it was cancelled after an 18-minute queue and requeued unchanged.
Queue delay is outside the job's 15-minute execution ceiling. There is no runtime-speed claim.

The SDK source change invalidated the earlier repository-task report. The first full PR gate
caught that stale roadmap state. [Run 37978664543](https://github.com/e6qu/fun-refactor/actions/runs/37978664543)
revalidated the two existing full-repository tasks and their 365 behavior cases, delivery,
interruption and patch replay. That
[report](../tests/agent-eval/results/2026-10-09-workflow-routes-python-repositories/result.json)
restored the same 7/18 demonstrated items, with no new milestone claimed.
Tests and the report-refresh workflow read that path from the catalog instead of duplicating a date.

## Preparing body edits and recovering failed checks

`AgentGuide.source_body_action` accepts caller-authored source keyed by every exact guided handle.
It copies the bodies into the existing typed task operation and reuses retained checks, delivery
and targets. It derives exact file/edit required conditions after the edit. It performs no discovery or subprocess work;
the existing guide, review and execution calls remain. Names and positions cannot substitute for handles.

[Run 37995201079](https://github.com/e6qu/fun-refactor/actions/runs/37995201079) passed the SDK type
check, focused tests and all 40 comparison cells. The [report](../tests/agent-eval/results/2026-10-10-guided-body-inputs/guide-body-context.json)
retains the complete executed caller programs, responses, source states and recovery receipts.
The same artifact refreshes the earlier 38-cell route comparison and scalar program evidence.
[Run 37995212729](https://github.com/e6qu/fun-refactor/actions/runs/37995212729) also refreshed the
two Python repository tasks and all 365 behavior cases. The catalog now selects its
[report](../tests/agent-eval/results/2026-10-10-guided-body-types-python-repositories/result.json);
the roadmap remains at 7/18 demonstrated items and 0/4 completed milestones.

| Prescribed edit | Explicit task construction | Guide body helper | Internal bytes, either arm |
| --- | ---: | ---: | ---: |
| One Python body | 1,677 | 1,082 | 19,089 |
| One Rust body | 1,676 | 1,081 | 18,753 |
| Two Rust bodies | 1,766 | 1,171 | 22,156 |
| Two Python files, same function name | 1,782 | 1,187 | 23,003 |

The middle columns count complete program UTF-8 (the text encoding used to count source bytes) bytes plus the canonical submission. The helper
removes 595 bytes in each fixture. Both arms make three fr calls with equal internal byte counts,
produce identical patches and pass all eight apply, check, reversal and delivery stages and independent behavior checks.
These are prescribed edits with known replacement bodies. No tokenizer, model, billing, time or
general autonomous-efficiency comparison ran.

The remaining 32 cells cover changed source/check declarations, a failed applied-state check and
a later conflicting edit. Applied failures return a nonzero exit with the structured transaction
report: source stays changed, later apply, check, reversal and delivery stages remain pending and no patch is delivered.
A reopened client inspects and previews undo of that transaction. Undo restores the exact original
source and the declared check passes; a conflicting later edit instead makes undo refuse without
overwriting it. This establishes undo recovery, not repaired delivery or automatic rollback.

Eight offline integrity tests reject omitted cells, incorrect byte totals, concealed applied source,
unearned patches, incorrect transaction recovery and overwritten conflicts. Native regression tests
also cover a clipped diff, ambiguous selection and missing source permission before history is saved.
We kept every public route and left frozen runner snapshots unchanged.

The later [recovery comparison](workflow-recovery.md#retained-results) extends applied-state
failure through checked delivery and patch application and checks in another checkout. It compares explicit resume with
undo-and-retry after repairing an external check prerequisite. Source changes still require
a new reviewed transaction; the comparison does not establish combined source-repair delivery.
