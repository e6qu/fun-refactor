# Complete fr for code analysis, graphs, refactoring and checked delivery

Give a smaller coding agent a concrete change or property to prove.
Fr should help it find the right code, prepare a precise edit, check its work and deliver the result within explicit limits.

The destination is the complete fr feature set: code and call graphs, code analysis,
refactoring, targeted changes, translation, proofs, recovery and public interfaces.
The smaller-agent milestones validate that product. They do not replace its broader scope.
The [proposed PR sequence](#proposed-pr-sequence) restores that scope and defines completion work.

## The three jobs

| Job | Example | Required check |
| --- | --- | --- |
| Refactor without changing behavior | Rename a Python function and update its imported callers | The promised callers change, unrelated names stay intact, and behavior checks still pass |
| Change behavior deliberately | Fix a Unicode comparison or add a function that counts escaped text | Tests reject the original mistake and accept the requested result |
| Write a Lean proof | Prove that Euclid's GCD code stops and returns the correct result for every input | Lean accepts both proofs of the unchanged executable; examples include `gcd(48, 18) = 6` |

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

Freeze client versions, model IDs, reasoning settings and limits before a live comparison.
Use fresh application state for every attempt and reject model substitutions.
Scripted client checks test setup; live attempts test whether an agent can finish the task.
See the [agent comparison policy](docs/product-review.md#compare-the-selected-agents).

## Next work

Deliver each complete milestone in one PR: implementation, examples, independent checks,
required live attempts, results and tracking belong together. Keep a milestone open until
its completion conditions are met and its CI passes. M1 and M2 have their complete comparisons.
M3 is open: GLM and Kimi still lack accepted page-count proofs within the original 150 seconds.

The seventh page-count study delivered four accepted proofs within its separate 300-second limit.
PR #619 has landed product-order facts in `proof start`; its live effect remains unmeasured.
Keep both observations separate from the original M3 completion gate.

### Proposed PR sequence

Each PR includes implementation, examples, checks, results and documentation.
The numbers below describe the work order, not GitHub PR numbers.
PR 1 landed in [#621](https://github.com/e6qu/fun-refactor/pull/621) with passing hosted checks.
PR 2 landed in [#623](https://github.com/e6qu/fun-refactor/pull/623) with passing hosted checks and refreshed evidence.
Its [coverage guide](docs/refactoring-coverage.md) tracks the operation families and their checks.
PR 3 merged in [#624](https://github.com/e6qu/fun-refactor/pull/624), covering proof authoring and bounded Rust scalar connections.
The [proof coverage guide](docs/proof-coverage.md) separates product checks from historical agent outcomes.
The original M3 timing gate remains open.
PR 4 merged in [#625](https://github.com/e6qu/fun-refactor/pull/625) with passing hosted checks and refreshed evidence.
The [interface coverage guide](docs/interface-coverage.md) tracks browser review, recovery, graph reporting and cross-interface delivery.
PR 5 is in progress in [#628](https://github.com/e6qu/fun-refactor/pull/628).
The [comparison protocol](tests/agent-eval/product-validation/README.md) reserves separate initial and validation tasks.
The user authorized at most 96 live attempts. Task checks and native admission must pass before calls.

| PR | Complete outcome | Completion gate |
| --- | --- | --- |
| 1. Complete graphs and code analysis | Find code and callers, trace values and effects, and explain what a change affects | A feature inventory, graph checks and independent runtime checks cover the promised languages and report unresolved behavior |
| 2. Complete refactoring and transformations | Discover affected code, preview a coordinated change, check it, undo it and deliver it | Every promised operation and language combination has positive, refusal, preservation and replay checks, including cross-language and framework cases |
| 3. Complete proof authoring and bounded implementation connections | Write, repair, maintain and deliver proofs, with checked source connections for one existing and one new function | All M3 cases have current evidence; original timing failures remain visible; the two implementation connections pass independent checks |
| 4. Complete recovery and public interfaces | Resume analysis and changes consistently through CLI, Python, WASM/browser and declared integrations | Each promised interface supports its assigned workflows, detects stale state and passes interruption, recovery and delivery checks |
| 5. Validate unfamiliar tasks and reduce complete costs | Establish where the full tool helps agents and remove measured overhead | Frozen matched trials retain all outcomes; improvements survive independent tasks without weaker correctness or accounting checks |
| 6. Qualify the complete product and release | Ship an integrated, documented feature set with reproducible evidence | No promised matrix cell remains missing; full hosted CI, artifact checks, examples and release replay pass |

#### PR 1: Complete graphs and code analysis

The [analysis inventory](docs/analysis-coverage.md) links current features to their checks.
Complete the inventory of commands, catalogs, language readers, resolvers, SDK methods,
browser features, recipes and documentation promises. Turn it into a maintained feature coverage matrix.
Each row names the feature, supported language forms, public interfaces, evidence and remaining work.
Distinguish implemented, independently checked, missing and intentionally unsupported cases.
Existing advertised features remain obligations. Removing a promise requires an explicit scope decision;
marking it unsupported cannot satisfy the completion gate.

Complete the graph gaps this inventory exposes.
Cover symbol and occurrence identities, lexical scope, imports, aliases, direct and candidate calls, recursion, dependency edges and consumer traversal.
Connect code graphs, call graphs and project maps through consistent source locations and query results.
Cover declared cross-language links, configured entry points and relevant build/configuration inputs.
Test shadowing, duplicate names, ambiguous dispatch, missing imports and external consumers.
Dynamic or unresolved relationships must retain their uncertainty through graph export and impact analysis.
Changing code or configuration must invalidate affected graph results.
Use independent fixtures and real projects; a graph display alone does not complete graph semantics.

Use the matrix to finish promised control-flow and data-flow operations across their declared language profiles.
Cover evaluation order, branches, loops, calls, returns, recursion, exceptions, aliases and shared mutation
where those profiles promise them. Complete the deferred Python invalid-number-input and shared-list cases.
Include declared source/sink rules, context-specific sanitizers and exact effect locations.
Connect analysis answers to graph occurrences, compiler settings and the evidence behind each conclusion.

Check reusable rules against independent execution and compiler results, including renamed and relocated examples.
Exercise original imports and package initialization rather than relying only on extracted functions.
Unknown calls, unsupported constructs and exhausted budgets must remain visible to downstream edits and proofs.
Parsing a language never substitutes for completing its promised analysis operations.
Update the earlier language-rule obligations only when their exact acceptance conditions pass.

#### PR 2: Complete refactoring and transformations

Complete every operation promised by the inventory, including rename, signature and parameter changes,
structural/body edits, batch operations, recipes and their declared language combinations.
Use graph and analysis results to discover callers, imports, contracts, configuration and relevant tests.
Preserve comments, formatting, unrelated names, argument evaluation and untouched source as required by each operation.
Ambiguous references and effectful argument removal must refuse unsafe edits.

Include declared translation, configuration/markup, cross-stack and framework/application transformations.
Preserve their explicit arithmetic, evaluation-order, runtime and adapter contracts.
For each family, deliver complete examples with previews, behavior checks, stale-plan refusal,
exact undo/redo and patch replay in another checkout. Test competing edits and partial failures.
Retain M2 regressions and report external consumers that the workspace cannot establish.
Whole-project migration remains a separate scope decision wherever the current contract excludes it.

#### PR 3: Complete proofs and implementation connections

Consolidate fact selection, goal inspection, bounded diagnostics and one-call delivery into one qualified proof workflow.
Include #619's product-order facts, rejected-save diagnostics and operation-based fact selection without benchmark-specific rules.
Recheck the eight Boolean language cases, both numeric tasks, protected claims, stale proof receipts
and preservation of unrelated handwritten proofs on the final implementation.

Report the status of each M3 requirement with the original 150-second comparison
and the separate 300-second follow-up side by side. The original gate stays open unless its conditions hold.
Agree any new completion rule before running the comparison.
Do not rerun the full 36-attempt study to seek a winner or silently extend existing limits.
Use the smallest declared diagnostic needed for an unresolved question; report negative results as outcomes.

Add a checked implementation/model connection for the existing Rust page-count function and new `fits_window` function.
Retain source, model, arithmetic semantics, toolchain, proof and trusted components for each.
Boundary tests and source hashes alone cannot establish that connection.
Changes to source, model, property or dependencies must invalidate the corresponding claim.
This addresses the earlier existing/new implementation proof obligations without promising arbitrary-program verification.

#### PR 4: Complete recovery and public interfaces

Finish matrix gaps in the CLI, Python SDK, WASM/browser, agent skills and declared integrations.
Make graph queries, analysis, previews and checked changes available wherever their public contracts promise them.
Document intentional interface differences; do not imply that a browser can run an unavailable native compiler.
Keep human-readable results and structured schemas consistent about uncertainty and completion.

Exercise saved findings, dependency-aware reuse, restart, stale review rejection, interrupted writes,
Git/history operations and delivery across revisions. Verify reused results against fresh analysis.
Check installable artifacts and runnable examples, including cross-interface handoffs where promised.
Refresh affected evidence with its original provenance intact. Avoid a second competing workflow or compatibility layer.

#### PR 5: Validate unfamiliar work and reduce overhead

Use complete refactoring, analysis/graph investigation, behavior-change and proof tasks to evaluate the integrated product.
Reserve unread projects and independent proof examples before implementation inspects them.
Freeze task checks, profiles, tool setups, repetitions, order, costs and decision rules before model calls.
Compare the same agent with ordinary tools and with fr, including setup, reading, retries, checks and delivery.
Separate correctness, complete-attempt success, matched successful times and spending per success.
Keep missing usage and charges unknown, and keep every failed or unstarted slot.

Use measured bottlenecks to simplify discovery, instructions, checking or delivery within this same PR.
Validate changes on a separately reserved set; do not tune on the final evaluation tasks.
Preserve the initial results and report regressions. Two failed diagnostic cycles trigger a review of the hypothesis.
Do not introduce proof-graph presentation or a proof-search engine without evidence that it addresses an observed obstacle.
List reversal stays reserved under the existing transfer decision; choose other examples for unrelated questions.
This work does not silently restart or satisfy the earlier CodeGraph or 48-attempt study.
If no benefit emerges, publish the negative result and an explicit product decision before release claims.

#### PR 6: Qualify and release the complete product

Check each feature against the shipped tool, tests, examples and documentation.
Close promised gaps; retain explicit unresolved obligations rather than describing a partial matrix as complete.
Run the full hosted test gates, language and interface checks, independent patch/proof checks,
installation tests and release replay against the final candidate.
Reconcile README, CLI/API references, skills, website, roadmap, bug list and handoff with the same support contract.
Preserve original study records and refresh only current evidence whose inputs changed.
Publish bounded claims about supported features and measured agent outcomes, including negative results.

All PRs keep local work serial, guarded and within the existing resource limits.
Full builds, full gates and large evidence regeneration run on GitHub.
A 300-second study does not authorize extending the 180-second local guard;
longer work requires a separately admitted execution environment. A resource refusal stops local work.

### Evidence behind the sequence

M1 and M2 have complete comparisons. Their reports include failed attempts and show no general time or cost savings.
The original M3 comparison accepted 28 of 36 deliveries. GLM and Kimi did not finish page counting within 150 seconds.
The later [page-count study](tests/agent-eval/proof-page-count/local-results/summary.json) delivered four accepted proofs within its separate 300-second limit.
The latest tool change, #619, supplies product-order facts; its effect on live delivery remains unmeasured.

The [Euclid study](tests/agent-eval/proof-algorithms/README.md) accepted eight of twenty deliveries and found no general advantage from proof diagrams.
The [argument-repair diagnostic](tests/agent-eval/proof-argument-repair/README.md) delivered no patch in eight attempts.
These results guide the next experiments. They do not limit the product's code or call graphs.
Detailed attempts, fixes and original limits remain in those reports and Git history.

### M1. Run the selected agents on three complete tasks

The [36-attempt comparison](tests/agent-eval/selected-agents/local-results/README.md) is complete: 27 passed and nine failed.
It covers a Boltons rename, Unicode repair and access-rule proof with and without fr.
These familiar tasks establish a baseline, not general agent savings. The report retains every outcome and submitted patch.

### M2. Make multi-file refactorings and targeted changes dependable

The [48-attempt comparison](tests/agent-eval/project-edits/local-results/README.md) is complete.
All patches passed independent checks; 43 attempts finished within 150 seconds and five timed out.
Every selected agent delivered an accepted result for each task using fr.

The tasks cover Rust parameter removal, a TypeScript export rename, escaped-text length APIs and a Python invoice repair.
Checks cover behavior, preserved source, effectful arguments, conflicting edits, delivery and exact undo/redo.
Matching successful attempts took 33% longer with fr for Luna, 50% for GLM and 17% for Kimi.
Earlier iterations and all 192 attempts remain in the [task reports](tests/agent-eval/project-edits/README.md).

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

## Scope and deferred work

The full advertised feature set is in scope for the proposed sequence.
PR #621 added the bounded invalid-number-input and shared-list expansions described in the analysis guide.
Analysis, graphs, refactoring and declared translation support remain product goals alongside proof authoring.
Preserve working behavior and fix encountered defects within the relevant complete PR.

Arbitrary-language semantics, unrestricted dynamic resolution, general verified translation,
new analysis engines and standalone cache optimization require a concrete uncovered obligation or measured blocker.
Add features to meet a concrete requirement. Do not drop a promised feature to close the matrix.

The former CodeGraph comparison and forty-eight-attempt broad study retain their original plans and outcomes.
They do not define completion of this PR sequence and are not restarted.
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

