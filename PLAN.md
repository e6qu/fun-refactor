# Help smaller agents refactor code, make changes and write Lean proofs

Give a smaller coding agent a concrete change or property to prove.
Fr should help it find the right code, prepare a precise edit, check its work and deliver the result within explicit limits.

## The three jobs

| Job | Example | Required check |
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
| OpenCode2 | `zai-coding-plan/glm-5.3-flash` | High reasoning in M1/M2; low reasoning in M3 |
| OpenCode2 | `kimi-code-plan-global/k3` | Non-Pro Kimi K3; high reasoning in M1/M2 and low reasoning in M3 |

The installed catalogs checked on 2026-10-25 contain these selections and also list Kimi K3.1 Pro.
Catalog entries and configured credentials do not establish that a live attempt will be admitted.
Freeze client releases, model identities, reasoning settings, accounting limits and attempt limits before live calls.
Use a fresh application state for each attempt. Reject a model substitution; do not inherit a personal default or use a stronger helper model.

The selected releases have six scripted native checks, one per profile and tool setup.
Those checks execute real commands with scripted replies; they test the setup rather than model task success.
The [M1 results](tests/agent-eval/selected-agents/local-results/README.md) record all thirty-six local CLI attempts,
their independently checked patches, failures and observed overhead against the same agents using ordinary tools.
Fast is a Codex service setting, not another Luna model.
See the [agent comparison policy](docs/product-review.md#compare-the-selected-agents).

## Next work

Deliver each complete milestone in one PR: implementation, complete examples, independent checks,
all required live attempts, results and tracking updates belong together.
Client setup or scripted checks alone are progress within that PR, not separate milestone PRs.
Keep a milestone open until its completion conditions are met and its CI passes.
M1 and M2 now have their complete comparisons.
PR #588 repaired the merged M3 implementation and passed all seven PR workflows.
Finish M3 on `milestone/complete-proof-authoring`, including the merged 0.86.1 release update.
The [task guide](tests/agent-eval/proof-writing/README.md) owns the fixed claims, checks and publication limits.
The [latest comparison](tests/agent-eval/proof-writing/local-results/README.md) retains all 36 attempts:
28 submitted a patch and eight timed out without one. [Independent grading](https://github.com/e6qu/fun-refactor/actions/runs/41062241970)
accepted all 28 patches. All three agents checked and saved accepted access and range proofs with fr.
GLM and Kimi submitted no page-count proof in either setup.

M3 remains open. The complete comparison stays paused while we improve the experiment process.
Follow the [improvement method](docs/agent-improvement.md): observe, explain, test a prediction, decide and review the method itself.
GLM and Kimi each completed four of six fr-available attempts in the first and latest complete comparisons.
Neither delivered a page-count proof in fourteen fr-available attempts across seven complete comparisons.
Using fr more consistently has not established a sustained rise in total success.
For example, eleven items at five per page need three pages; valid inputs must never produce more pages than items.
The [behavior review](tests/agent-eval/proof-writing/behavior-review.md) separates searches, actual checks and delivered patches.

The [first controlled diagnostic](tests/agent-eval/proof-feedback/README.md#what-the-first-run-established) ran all eight attempts once.
All eight submitted proofs passed independent checking. Six complete attempts passed; GLM timed out after passing checks,
and Kimi overwrote the supplied check settings. The stated rules give no demonstrated benefit from supplying the initial error report.
Review found that this diagnostic skipped the shared execution instructions. That defect is fixed and has a regression.
The later instruction correction has no live measurement yet. The 150-second limit stays unchanged.

The next tasks stay inside this milestone PR:

1. Finish hosted M3 verification of proof-reader repairs, warning rejection, dependency controls and original submissions.
   Keep the [audit](tests/agent-eval/proof-writing/report-audit.json) of twelve complete reports and 480 attempts unchanged.
   Qualify the restored shared instructions before another paid call.
2. Prepare a fresh Rust multiplication case, `scaled_total(a, b) = a * b`, with an empty proof region.
   Its fixed Lean claim is `a ≤ scaled_total(a, b)` when `0 ≤ a` and `1 ≤ b`.
   State that Lean uses unbounded integers; source overflow and the source/model connection remain unproved.
   Check an external reference and wrong proofs before calls. Keep that reference outside agent projects.
3. Freeze four fresh construction attempts: two each for the pinned GLM and Kimi profiles, all with fr and the corrected instructions.
   Record the first passing proof, package check, delivered patch and session finish separately.
   Each attempt must preserve check settings and finish within 150 seconds. Inspect any failure before another change or paid batch.
   Treat this as a workflow and construction check; it cannot establish that the instruction change caused an improvement.
   If agents still repeat checks or alter settings, inspect that first failure before choosing another instruction change.
4. Return to the first unresolved page-count proof step once that workflow check passes.
   Resume the complete comparison only after a concrete change has supporting evidence and relevant CI passes.
   Keep the same models, claims, two repetitions and 150-second benchmark.
   Independently grade submissions and preserve every failure alongside the 335 existing M3 outcomes and this separate diagnostic.

Two diagnostic cycles that leave the same obstacle unchanged require a review of the explanation and task requirements.
Do not repeat full comparisons until one passes. Keep all earlier records and their timing limitations intact.

### M1. Run the selected agents on three complete tasks

The [36-attempt comparison](tests/agent-eval/selected-agents/local-results/README.md) covers a Boltons rename,
a Unicode repair and a Lean proof of the fixed staff-or-owner access model.
It retains every outcome and submitted patch. A timed-out client stays failed even when its patch passes.
Only patches and summaries are public; full CLI records and credentials remain local.
Actual charges and unavailable usage remain unknown. A checked model proof leaves its connection to source behavior open.

### M2. Make multi-file refactorings and targeted changes dependable

The [latest comparison](tests/agent-eval/project-edits/local-results/README.md) independently checked all 48 submissions.
All 48 patches passed. Forty-three attempts finished within the original 150-second limit; five timed out and remain failed.
Every selected agent delivered at least one accepted result per task using a reviewed, contributing fr step.
The complete milestone belongs in one PR, with its implementation, checks, examples, results and tracking.
The full PR CI gate must also pass.

| Completed case | Concrete behavior |
| --- | --- |
| Rust parameter removal | Remove an unused Boolean input across two packages and all five declared calls; preserve other argument evaluation and refuse removal of an argument with effects |
| TypeScript rename | Change the exported `price` to `quote` through an import alias; keep calls named `displayPrice` and preserve the unrelated `price` function |
| Regex feature | Add documented length APIs in both packages; count UTF-8 bytes without allocating, produce escaped text with at most one allocation, and preserve append behavior |
| Python repair | Make `invoice_total(5, 2)` return 7 while preserving the public signature and unrelated source |

Supplied controls also check preserved source, competing edits, failed-check diagnostics, separate patch delivery and exact undo/redo.
External callers that the workspace cannot identify remain unknown.
The [task guide](tests/agent-eval/project-edits/README.md) owns the exact frozen requirements and limits.

| Complete comparison | Accepted / 48 | What changed before its first call |
| --- | ---: | --- |
| [Original](tests/agent-eval/project-edits/first-results/README.md) | 33 | Fr was available; using it was optional |
| [Shorter guides](tests/agent-eval/project-edits/guided-results/README.md) | 40 | State the deadline, require a useful fr step, simplify examples and repair the local Rust helper |
| [Actual clock](tests/agent-eval/project-edits/clocked-results/README.md) | 41 | List requirements separately, give the actual finish time and reserve time for the response |
| [Behavior checks](tests/agent-eval/project-edits/local-results/README.md) | 43 | Check actual byte counts and allocations, avoid optimized development builds, and protect supplied checking files |

Keep all 192 attempts, including failures. These combined changes do not isolate one cause of the differences.
For matching successful pairs in the latest comparison, adding fr took 33% longer with Luna, 50% with GLM and 17% with Kimi.
The baseline is the same agent using ordinary tools. Times include preparation, replies, tools, checks and delivery.
These are code-change times, not execution-speed measurements of the changed programs. No general savings are established.

### M3. Help the agents write, repair and maintain Lean proofs

A property is the exact claim to prove. Tactics are the proof steps the agent writes for Lean to check.
The initial language cases declare their argument and result types.
They return values without external reads or writes. This is the typed pure subset used below.

1. Make the complete access-rule lesson runnable: source, generated model, unchanged property, one disclosed goal, agent-written tactics, Lean diagnostics and a checked saved proof.
   A scripted control may test this path; only a live attempt shows an agent writing the proof.
2. Repeat that Boolean lesson for the admitted typed pure forms in Rust, Python, Go, Java, TypeScript, TSX, Zig and Lean.
   Record a separate result for each language. A TSX helper here contains ordinary typed code, not a proof of JSX rendering.
   Parsing other languages or formats does not establish proof support.
3. Add these two numeric tasks with explicit arithmetic:

   Signed-64 integers range from -9,223,372,036,854,775,808 through 9,223,372,036,854,775,807.
   Unsigned-64 integers range from zero through 18,446,744,073,709,551,615.
   Lean's mathematical integers do not wrap at those boundaries; retain each source function's arithmetic rules with its model.

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
   Both tool setups start with the same source, definitions and fixed property.
   Two repetitions per profile, task and setup give thirty-six attempts.
   Count failed tactics, retries and complete costs. Record source-language limitations beside the outcome.

**M3 is complete when** the runnable proof-writing cases pass for every named admitted language.
Both numeric tasks need checked proofs under their declared models.
Each selected agent must write at least one accepted proof for each live task using fr within the frozen repetitions and limits.
Record which fr actions contributed to that accepted proof.
Keep failed tactics and unsuccessful attempts in the results.
An unchecked source/model connection remains an open claim in every report.
A general verified translator or proof of arbitrary programs is not a completion condition for this proof-authoring milestone.

## Work outside the active queue

The earlier invalid-number-input and shared-list expansions are no longer the next milestones.
General Python semantics, new analysis engines, broad translation expansion and standalone cache optimization are outside the active queue.
A concrete blocker from M1, M2 or M3 can bring the required work into scope.
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

Before choosing an operation, run `fr --json audit` and `fr capabilities` to check installed support.
Use fr on its own work where it fits. Fix encountered failures and timeout causes without increasing limits or dropping coverage.
Keep model choices, settings, limits and check selection explicit. Add no compatibility routes, automatic model substitutions or needless wrappers.
Builds, full tests and large collections run on GitHub under the [workstation rules](docs/development.md#working-on-a-shared-desktop).
Preserve original report hashes. Refresh records after their recorded inputs change.
Use the [handoff](docs/continuity.md) for current maintenance, [bug list](BUGS.md) for defects and [guide index](docs/README.md) for task instructions.

