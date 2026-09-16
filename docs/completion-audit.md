# Completion audit

`fr audit` gives an agent a small support and trust summary before it chooses a workflow. The
summary derives counts from live predicates and catalogs. It also gives exact commands for seven
bounded detail sections.

```sh
fr audit
fr --json audit workflows
fr --json audit boundaries
```

Every report carries `fr-completion-audit-1` and a Merkle object root. The summary reports parser,
capability, workflow, recipe and application counts. Detail sections expose the underlying rows,
schemas, commands, source policy, proof classes and refusal reasons.

The report keeps four questions separate:

- A support predicate says whether a route admits a language, adapter or feature class.
- An executable acceptance target says where the repository tests that route.
- A retained evaluation records what one fixed run observed.
- A proof names only its model, assumptions and checked correspondence boundary.

An unrun test remains an obligation. A supported route can refuse an exact input. A model theorem
does not establish runtime behavior or general source equivalence.

## Deterministic workflow sweep

`tools/completion-workflows.py` exercises understanding, tracing, direct change, recipe, semantic
edit, framework migration and proof guidance on an isolated fixture. Each route executes at least
one returned read or preview action. The tool verifies that these previews leave all source bytes
unchanged.

The retained [workflow report](../tests/agent-eval/completion-workflows.json) records fourteen real
manual discovery calls. The guide replaces them with seven structured goal calls and leaves zero
exploratory help, vocabulary, schema or audit calls after guidance. It records complete request and
response byte counts for each discovery and guide call.

The deterministic report does not run a model. It omits hidden reasoning, tokenizer counts and
billed quota. Fresh Codex evidence records those fields separately when the CLI exposes them.

Regenerate and verify the report with:

```sh
python3 tools/completion-workflows.py --fr target/debug/fr \
  --output tests/agent-eval/completion-workflows.json
python3 tools/completion-workflows.py \
  --audit tests/agent-eval/completion-workflows.json
```

The report binds the evaluator and implementing source files by SHA-256. Its auditor refuses stale
bindings, missing workflow families, source changes, absent action execution or overstated context
claims.

## Fresh agent cohort

`tools/completion-agent-eval.py` prepares two isolated sessions around a frozen `fr` binary. The
fundamentals session covers understanding, tracing, direct change and recipes. The structured
session covers semantic editing, framework migration and proof submission. Agents can call only
the instrumented guide, follow and finish operations.

The harness rejects source mutation, repeated actions, missing route families, output-schema drift
and any Codex command that bypasses its instrumented step. It retains the prompt, tool events,
Codex JSONL, stderr, final answer, source identities, exact model settings and score for failures as
well as successes. Codex JSONL supplies model-token usage. The report marks billed quota unavailable
because the CLI does not expose it.

Real sessions require the explicit spend flag. The agreed economical configuration remains
`gpt-5.6-luna`, low reasoning effort and the default service tier for this installed CLI.

```sh
python3 tools/completion-agent-eval.py prepare /tmp/fr-completion-agent \
  --fr target/debug/fr
python3 tools/completion-agent-eval.py run /tmp/fr-completion-agent \
  --confirm-agent-spend
python3 tools/completion-agent-eval.py score /tmp/fr-completion-agent
```

`record` accepts only a passing cohort. `replay` verifies the retained file hashes and both passing
session results without contacting Codex.

The first [retained diagnostic](../tests/agent-eval/results/2026-09-17-completion-diagnostic-1/manifest.json)
failed. Its fundamentals agent eventually completed four routes after repeated recipe grammar
guesses. Its structured agent copied an unrelated example, submitted the invalid `semantic` kind
and stopped. The follow-up adds the recipe file envelope to the live guide contract, removes the
unrelated example and counts every failed Codex command. The diagnostic remains non-acceptance
evidence.

The second [retained diagnostic](../tests/agent-eval/results/2026-09-17-completion-diagnostic-2/manifest.json)
eventually completed every route, migration preview and proof check. Both agents first made many
invalid goal-schema guesses, so the strict scorer rejected them. The final handoff now includes
exact task-specific goal objects. The guide also returns a target-specific recipe template.

The third [retained diagnostic](../tests/agent-eval/results/2026-09-17-completion-diagnostic-3/manifest.json)
completed every route with no direct project access, exploratory call, mutation or human
correction. Strict scoring rejected two malformed JSON transports and one proof action that did not
reattach its previously authored tactics file. Complete guide requests now remove JSON-envelope
reconstruction. Authored artifacts also remain addressable by their bounded plain names for later
actions in the same session.

The fourth [retained diagnostic](../tests/agent-eval/results/2026-09-17-completion-diagnostic-4/manifest.json)
completed every route with no failed completed command. Codex stderr nevertheless records one
internal tool-router process error, so the run remains diagnostic evidence. Scoring now reports
infrastructure errors separately and requires zero for acceptance. A diagnostic recording can
never become acceptance evidence, even when its workflow result otherwise passes.

The fifth [retained diagnostic](../tests/agent-eval/results/2026-09-17-completion-diagnostic-5/manifest.json)
again completed every route. It exposed one prompt typo in a recipe placeholder and one unrecorded
Codex tool-router failure. The prompt now requires byte-exact returned placeholders and complete
nonempty exec commands. Both failure classes remain independently visible in its retained score.
