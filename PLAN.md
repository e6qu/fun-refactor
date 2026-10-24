# Help smaller agents refactor code, make changes and write Lean proofs

Give a smaller coding agent a concrete change or property to prove.
Fr should help it find the right code, prepare a precise edit, check its work and deliver the result within explicit limits.

## The three jobs

| Job | Example | What must be checked |
| --- | --- | --- |
| Refactor without changing behavior | Rename a Python function and update its imported callers | The promised callers change, unrelated names stay intact, and behavior checks still pass |
| Change behavior deliberately | Fix a Unicode comparison or add a function that counts escaped text | Tests reject the original mistake and accept the requested result |
| Write a Lean proof | Show that an access function returns true exactly for staff or owners | Lean accepts the agent's proof of the unchanged property; the source/model connection and assumptions remain visible |

Prefer edits that retain comments, formatting and source outside the intended change.
Parsing successfully is a separate check. Preserving behavior is required for a refactoring; repairs and features deliberately change behavior.
A larger rewrite is allowed when the task needs it, with its full diff and checks.

A Lean model describes the admitted source behavior in Lean.
A proof of that model is useful, but does not by itself prove that the original program behaves the same way.
Reports must distinguish those claims.

## Target agents

| Application | Exact model selection | Required setting |
| --- | --- | --- |
| Codex | `gpt-6-luna` | Fast mode; check and record the effective client service tier |
| OpenCode2 | `zai-coding-plan/glm-5.3-flash` | Explicit model and reasoning settings |
| OpenCode2 | `kimi-code-plan-global/k3` | The configured non-Pro Kimi K3; explicit reasoning settings |

The installed catalogs checked on 2026-10-25 contain these selections and also list Kimi K3.1 Pro.
Catalog entries and configured credentials do not establish that a live attempt will be admitted.
Freeze client releases, model identities, reasoning settings, prices, spending caps and attempt limits before paid calls.
Use a fresh application state for each attempt. Reject a model substitution; do not inherit a personal default or use a stronger helper model.

The current scripted OpenCode controls use an older command format.
OpenCode2 integration is work to complete, not an already passing result.
Fast is a Codex service setting, not another Luna model.
See the [agent comparison policy](docs/product-review.md#compare-the-selected-agents).

## Next work

Deliver each milestone with its implementation, complete examples, independent checks and updated results.
All three milestones below are open. Start with M1.

### M1. Run the selected agents on three complete tasks

1. Pin the Codex and OpenCode2 releases and record their actual model lists.
   Implement OpenCode2's client adapter, tool-result collection and process shutdown.
   Test skill discovery, requested model/effort/speed, ordinary tools and fr access with scripted responses before live calls.
2. Freeze these three task requirements and their independent checks before changing support:

   | Task | Required result | Check that catches a wrong answer |
   | --- | --- | --- |
   | Rename Boltons' `clamp` to `clamp_value` in its declared project change | Update the function, named consumers and documentation | Limiting 70 to 0 through 50 still returns 50; an unrelated same-named function stays unchanged |
   | Repair the recorded `unicode-dice` comparison | Count the intended Unicode characters on both sides | The reported Unicode examples and the independent regression set pass |
   | Prove the Rust Boolean `allowed(staff, owner)` rule | Write a Lean proof that its model equals `staff || owner` | All four input pairs agree; changing the model to `staff && owner` cannot retain the same claim |

3. Use each of the three profiles with ordinary file tools, then with those tools plus fr.
   That is eighteen first attempts. Repeat the unchanged schedule once for thirty-six attempts in total.
   Rotate tool order. These are familiar development tasks, not new unread-project qualifications.
4. Give the model the request and checking commands, without the solution or edit location.
   Keep the requested proof property fixed. Count any supplied repair or proof as scripted help.
5. Record correct results, regressions, human intervention, fr use, all model work, retries, setup, indexing, checks, shutdown and failures.
   Keep unavailable billing data unknown.
6. Compare each profile against itself with ordinary tools. Correctness comes first.
   Report cost and time for equivalent successful outcomes and total spending per success.
   A tool that was offered but unused establishes no benefit from fr.

**M1 is complete when** the three client profiles pass model-free controls and all thirty-six live attempts have independently checked outcomes and complete or explicitly missing cost fields.
Failures count as results. Passing this small study establishes neither a general success rate nor general savings.
Model-free preparation can proceed now; live calls still need agreed settings and spending limits.

### M2. Make multi-file refactorings and targeted changes dependable

Freeze these four additional cases before implementing missing rules:

| Case | Concrete requirement |
| --- | --- |
| Rust parameter removal | Remove one unused parameter across a two-crate workspace; update every declared direct caller and report external or unresolved uses |
| Regex feature | Add the recorded allocation-free escaped-length feature; cover every required escaping class and Unicode input without constructing the escaped string |
| Python repair | Fix `invoice_total(amount, fee)` omitting the fee; `invoice_total(5, 2)` must return 7 |
| TypeScript rename | Rename an exported helper through an imported alias while leaving an unrelated local declaration with the old name untouched |

For every case:

- The agent finds the target from the request, reads the relevant code, previews one complete change and runs explicit project checks.
- Test preserved bytes outside declared edit ranges, comments, Unicode text and indentation.
  An intentionally wider rewrite must identify its wider range in the preview.
- Inspect affected imports, declared callers, tests and relevant configuration. Report omissions and unresolved uses.
- A competing source edit makes the old preview refuse before writing. Refresh the review from current source.
- A failed check preserves its diagnostics and the actual edit state.
- Undo restores exact original bytes; redo restores the reviewed change.
  The exported patch passes independent checks in another checkout.
- Give each target agent one documented route for each task. Reuse existing edit/check machinery and remove redundant choices instead of adding another wrapper.
- Run two repetitions per case, profile and tool setup under frozen settings: forty-eight attempts.
  Preserve the earlier attempts and compare matching outcomes.

**M2 is complete when** all four cases pass the scripted delivery checks.
Each selected agent must also deliver at least one independently accepted result per case within the frozen repetitions and limits.
Keep the failed outcomes alongside those successes; a scripted success alone cannot close the agent requirement.
An unsupported construct remains an explicit refusal until its required reusable rule is implemented.
Add Python analysis or other language rules only when one of these code-change tasks needs them.

### M3. Help the agents write, repair and maintain Lean proofs

A property is the exact claim to prove. Tactics are the proof steps the agent writes for Lean to check.

1. Make the complete access-rule lesson runnable: source, generated model, unchanged property, one disclosed goal, agent-written tactics, Lean diagnostics and a checked saved proof.
   A scripted control may test this path; only a live attempt shows an agent writing the proof.
2. Repeat that Boolean lesson for the admitted typed pure forms in Rust, Python, Go, Java, TypeScript, TSX, Zig and Lean.
   Record a separate result for each language. A TSX helper here contains ordinary typed code, not a proof of JSX rendering.
   Parsing other languages or formats does not establish proof support.
3. Add these two numeric tasks with explicit arithmetic:

   | Function | Property and boundary cases |
   | --- | --- |
   | Existing Rust [page count](tests/agent-eval/checked-translation/pages.rs) | Eleven items at five per page need three pages; negative item counts or nonpositive sizes return `-1`; valid large inputs avoid signed-64 overflow |
   | New Rust `fits_window(offset, count, total)` | Unsigned-64 inputs; an empty range at the end is valid; ranges past the end are invalid; the guard `offset <= total && count <= total - offset` avoids subtraction overflow |

4. Let the model repair rejected tactics using bounded, relevant Lean diagnostics.
   Keep the theorem, definitions and assumptions fixed during tactic repair.
   Reject `sorry`, `admit`, new unapproved axioms and replacement claims that weaken the requested property.
5. Record source, generated definitions, arithmetic rules, proof, toolchain and declared trusted components.
   Report model proofs separately from checked connections to source behavior.
   A source hash or matching signature alone cannot close the source-implementation claim.
6. Change a source comparison, generated model, property and relevant dependency in separate tests.
   Old proof receipts must expire; a newly attempted proof of a false fixed property must fail.
   Preserve unrelated hand-written proof regions when regenerating supported definitions.
7. Run the selected agents on the Rust Boolean and two numeric tasks with and without fr.
   Two repetitions per profile, task and setup give thirty-six attempts.
   Count failed tactics, retries and complete costs. Record source-language limitations beside the outcome.

**M3 is complete when** the runnable proof-writing cases pass for every named admitted language and both numeric tasks have checked proofs under their declared models.
Each selected agent must write at least one accepted proof for each of the three live tasks within the frozen repetitions and limits.
Keep failed tactics and unsuccessful attempts in the results.
An unchecked source/model connection remains an open claim in every report.
A general verified translator or proof of arbitrary programs is not a completion condition for this proof-authoring milestone.

## Work outside the active queue

The earlier invalid-number-input and shared-list expansions are no longer the next milestones.
General Python semantics, new analysis engines, broad translation expansion and standalone cache optimization need a concrete blocker from M1, M2 or M3 before becoming active work.
Keep working behavior and fix encountered defects; this change does not remove existing capabilities.

The former CodeGraph comparison and forty-eight-attempt broad study retain their original plans and outcomes.
They do not define completion of this focused roadmap and are not restarted.
The [earlier checklist](docs/earlier-roadmap-checklist.md) and [retained-results report](docs/roadmap-status.md) preserve unfinished earlier claims without marking them complete.

## Generalization requirements

- Fix task requirements and independent checks before extending support.
- Keep production behavior independent of benchmark names, paths, revisions and expected patches.
- Include positive, negative, ambiguous and unsupported cases. An incomplete answer cannot justify an absence claim.
- Separate familiar development tasks from new unread-project attempts.
- Keep all failures, timeouts, cancellations and unstarted work. Never retry until success and report only the winner.
- Require passing compiler/behavior checks for changes and Lean checks for proof claims.
- Measure complete attempts, including tool instructions and background work, against the same agent using ordinary tools.

## Working rules

Use fr on its own work where it fits. Fix encountered failures and timeout causes without increasing limits or dropping coverage.
Keep model choices, settings, limits and check selection explicit. Add no compatibility routes, automatic model substitutions or needless wrappers.
Builds, full tests and large collections run on GitHub under the [workstation rules](docs/development.md#working-on-a-shared-desktop).
Preserve original report hashes. Refresh records after their recorded inputs change.
Use the [handoff](docs/continuity.md) for current maintenance, [bug list](BUGS.md) for defects and [guide index](docs/README.md) for task instructions.

