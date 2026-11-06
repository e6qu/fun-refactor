# Help smaller agents refactor code, make changes and write Lean proofs

Give a smaller coding agent a concrete change or property to prove.
Fr should help it find the right code, prepare a precise edit, check its work and deliver the result within explicit limits.

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

Deliver each complete milestone in one PR: implementation, examples, independent checks,
required live attempts, results and tracking belong together. Keep a milestone open until
its completion conditions are met and its CI passes. M1 and M2 have their complete comparisons.
M3 is open: GLM and Kimi still lack accepted page-count proofs within the original 150 seconds.

The [improvement method](docs/agent-improvement.md) requires a recorded obstacle, a specific
proposed repair, checked controls and a comparison declared before any model calls.
The latest evidence gives us three useful limits:

| Study | What happened | What it establishes |
| --- | --- | --- |
| [Write the three Rust model proofs](tests/agent-eval/proof-writing/local-results/README.md) | 28 of 36 attempts delivered accepted patches; eight timed out. GLM and Kimi delivered no page-count proof | Fr can support access and range proofs; page count remains unfinished |
| [Write the actual Euclid algorithm proofs](tests/agent-eval/proof-algorithms/README.md) | Eight of twenty attempts delivered accepted patches; twelve timed out | Two repetitions show no general graph advantage; no transfer test was earned |
| [Repair page-count arguments](tests/agent-eval/proof-argument-repair/README.md) | All eight attempts timed out without a patch. One Kimi draft passed checking near 135 seconds but was not saved or delivered | The predicted argument mistake was not observed, so this does not validate that repair instruction |

Every failure, interrupted collection and unstarted slot remains recorded. A correct draft,
a checked proof and a delivered patch are distinct outcomes. Unknown usage and charges stay unknown.
The [supplied-proof delivery study](tests/agent-eval/proof-algorithms/local-results/M3-delivery/README.md)
measures delivery of given answers; it cannot replace proof discovery.

**Completed:** [#603](https://github.com/e6qu/fun-refactor/pull/603) merged the explicit
`fr proof start`, `state`, `check` and `save` workflow, dependable JSON errors and Lean reader repairs.
Its [92-job CI gate](https://github.com/e6qu/fun-refactor/actions/runs/42085550950)
and all companion workflows passed. These tool checks establish no improvement in GLM or Kimi success.

**PR #604: distinguish finding division facts from applying them.**

The hosted controls pass for the current tool. They check real proofs, reject wrong
patches and protect the supplied skill. The refreshed project replay also passes
with release `0.89.2`. Fr now ignores its own generated history and cache when
identifying program source. Earlier records keep their original hashes and outcomes.
The complete PR gate and four local client checks still precede the eight calls.
No new model attempt has started; M3 remains open.

1. Refresh the [eight-attempt diagnostic](tests/agent-eval/proof-division-facts/README.md)
   for the current commands and OpenCode2 2.2.5. Prove that a quotient plus its remainder
   is at most the nonnegative input when the divisor is positive.
   For example, eleven divided by five gives two plus one, and three is at most eleven.
2. Give both conditions the same theorem, tools, skills and 150-second complete-attempt limit.
   One condition receives four current library declaration types; the other must select facts
   from the same installed library. Neither receives a completed proof.
3. Pass independent hosted proof and patch controls, then all four complete local client checks.
   Cover actual goal inspection, rejected tactics, saving, package checks, delivered patch replay
   and final accounting. Keep the resource reserve and observe detached workers.
4. Freeze the eight slots before calls. Run each once; independently grade every original patch.
   Report complete successes, timeouts, usage and unknown charges. Review the first proof check,
   first accepted proof, save and delivery. Make the per-agent decision fixed in the protocol.

The earlier argument-repair instruction produced no delivered patch and its predicted error was
unobserved. This diagnostic tests a different obstacle; it cannot close M3 or establish a benefit
from graph presentation. A resource refusal blocks launching, rather than permitting weaker checks.
Do not repeat the full 36-attempt study to seek a passing result. Keep list reversal reserved.
The named language cases, both numeric tasks and source/model checks below still define M3 completion.

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

