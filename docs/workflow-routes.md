# Which editing workflow should an agent use?

The next comparison measures the cost of reaching the same checked edit through four public
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

Run on GitHub, with a 15-minute job ceiling and no OpenCode or provider calls:

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

Results are pending. Existing measurements remain historical evidence, not results of this run.
