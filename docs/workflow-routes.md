# Which editing workflow should an agent use?

This comparison measures the cost of reaching the same checked edit through four public
routes. It runs prescribed programs, without a model. Calls and serialized bytes describe those
programs; they do not establish autonomous agent efficiency or billed token savings.

## What each route contributes

| Route | Use it when | Checks and recovery | Existing consumers |
| --- | --- | --- | --- |
| `guide` → reviewed intent | The agent has a structured goal and needs an admitted operation | Binds the goal, target, allowed operation, source permission and check/delivery policy; native intent revalidates them before writing | `FrClient.review_guide`, `tests/fixtures/tagged_intent_sdk.py`, `tools/upstream-multibody-agent.py`, `tools/proof-authoring-agent.py` |
| `intent` | The operation is known and needs a bounded evidence packet | Binds selected evidence and the complete operation review; writes use the same checked lifecycle | `FrClient.compile`, `tests/agent_intent_cli.rs`, capability/recipe/migration/proof clients |
| `task-change` | Exact structural edits and named checks are known | Resolves request references, binds fragments and postconditions, checks original/applied/restored states, retains history and patch | `FrClient.review`, configured change runners, `tools/task-change-context.py` |
| `author batch` → `workflow` | The caller needs a separately saved plan or explicit transaction composition | Author validates disjoint edits and source freshness; workflow binds checks and delivery to the saved transaction | CLI authoring, `tools/author-batch-context.py`, task template consumers |

These are overlapping layers, not four interchangeable APIs. Guide and intent cover capabilities,
recipes, migrations and proof work beyond structural task changes. A saved direct-author plan is
also useful without delivery. Direct authoring does not attach required checks to the transaction;
its workflow enforces the chosen checks. Task and intent transactions retain that requirement.
No route is scheduled for removal based only on in-repository callers.

The instruction entry point is `skills/fr/SKILL.md`. Guide, intents, task and author references
each explain their own manifest and review identity. Task's manual template composition is still
needed before concrete fragments exist. Once inputs are complete, `task-change` removes those
joins. Do not duplicate all four manifests in the entry guide.

## Bounded comparison

Collected on GitHub, with a 15-minute job ceiling and no OpenCode or provider calls:

- Compare a Rust scalar edit, a Python body replacement and two coordinated Rust bodies.
- Use identical starting files, authored changes, postconditions, named checks, reversal and patch
  delivery in each route. Retain the commands, input payloads, response objects and refusals.
- Check exact final source and patch bytes, all eight lifecycle stages, and independent behavior.
  Report the direct-author transaction's different check binding rather than hiding it.
- Compare guided execution before and after consolidation using the exact former execution
  function. Keep its source and origin with the report. Count actual calls and canonical JSON
  request/response bytes; do not infer time or token savings from those counts.
- Exercise stale source, changed check declarations, changed fragment inputs, a failed original
  check and mutation of a retained guide goal/review. A refused execution must not modify source
  or deliver a patch. Keep any prepared history plan visible in the result.

The first consolidation candidate is the SDK's separate guide read immediately before intent
execution. Native intent already recomputes the guide in the edit snapshot. Remove the extra read
only while preserving local review integrity, including the retained goal, and demonstrating the
native refusals. Keep the separate refresh for read/preview guide actions, which have no native
guide binding.

The former execution function uses the current review objects and native binary. This isolates
the extra guide call; it is not a replay of the entire former SDK. Every arm uses the same
temporary path and a fresh project/history. External fragment bytes are retained separately;
request counts include file-backed manifest contents. Common fixture setup and independent
oracle processes are outside the fr traffic count.

## Results

[Run 37973505951, attempt 2](https://github.com/e6qu/fun-refactor/actions/runs/37973505951)
passed all 38 cells: 15 successful edits and 23 refusals. The
[retained report](../tests/agent-eval/results/2026-10-09-workflow-routes/result.json) contains
every command and response; its manifest records the source commit, binary digest and original
GitHub artifact digest. Seven offline regression tests check completeness and corruptions.

Each successful arm produced identical final source and patch bytes and passed all eight
lifecycle stages plus an independent behavior check. Cells with changed source, checks or
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
known; use the guide when selecting an admitted operation. The next candidate is repeated body-input
preparation, with recovery after an applied-state check failure still needing a direct comparison.

Two development runs failed: the first expected the wrong scalar formatting; the second compared
an in-memory tuple with a JSON list during auditing. Both remain visible in GitHub
([37972312155](https://github.com/e6qu/fun-refactor/actions/runs/37972312155),
[37973025301](https://github.com/e6qu/fun-refactor/actions/runs/37973025301)). The final run's first
attempt never acquired a runner; it was cancelled after an 18-minute queue and requeued unchanged.
Queue delay is outside the job's 15-minute execution ceiling. There is no runtime-speed claim.
