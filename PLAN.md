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

Deliver each complete milestone in one PR: implementation, complete examples, independent checks,
all required live attempts, results and tracking updates belong together.
Client setup or scripted checks alone are progress within that PR, not separate milestone PRs.
Keep a milestone open until its completion conditions are met and its CI passes.
M1 and M2 now have their complete comparisons.
PR #588 repaired the merged M3 implementation and passed all seven PR workflows.
Finish the remaining M3 work together, starting from merged main.
The [task guide](tests/agent-eval/proof-writing/README.md) owns the fixed claims, checks and publication limits.
The [latest comparison](tests/agent-eval/proof-writing/local-results/README.md) retains all 36 attempts:
28 submitted a patch and eight timed out without one. [Independent grading](https://github.com/e6qu/fun-refactor/actions/runs/41062241970)
accepted all 28 patches. All three agents checked and saved accepted access and range proofs with fr.
GLM and Kimi submitted no page-count proof in either setup.

M3 remains open: GLM and Kimi still lack accepted page-count proofs within the
original 150-second limit. The [method](docs/agent-improvement.md) and
[behavior review](tests/agent-eval/proof-writing/behavior-review.md) explain why
more tool use alone has not established an improvement.

The [Euclid study](tests/agent-eval/proof-algorithms/README.md) is now complete:
all twenty fresh sessions ran once, and independent GitHub checks accepted all
eight delivered patches. Twelve complete attempts timed out. The executable
function and its termination and correctness claims stayed fixed.

| Comparison | Observed complete results |
| --- | --- |
| Select facts / receive declarations | GLM 0/2 in each; Kimi 0/2 and 1/2 |
| Checklist / identical-information graph | Luna 2/2 and 2/2; Kimi 1/2 and 2/2; GLM 0/2 in each |

One GLM presentation attempt encountered a confirmed fr reader defect, so we cannot rely on its
comparison. Two repetitions establish no general graph
advantage. No agent met the planned two-versus-zero rule for a transfer test;
accumulator list reversal stays reserved. The results do not yet justify an
automatic controller that tries alternative proof paths.

**Merged in PR #592:** the reader accepts multiline GCD proofs and
keeps their following tactic and declaration separate. Both valid references pass;
all twelve deliberately broken controls fail. The original checker independently
grades all twenty sessions. Repaired fr accepts the eight delivered patches and
checks the misplaced ninth diff separately without counting a new success.
The small proof guides address the observed flag, context and delivery mistakes;
these instruction changes have no measured success gain yet.

Both fixed benchmark corpora now pass comparisons with every tracked file
present, with no changed symbols or references. The
[audit summary](tests/agent-eval/index-consumers/lambda-reader-control-summary.json)
records both runs and preserves the original timing baselines. Refreshed reports
and tracking pages are retained. The earlier
[five sessions](tests/agent-eval/proof-algorithms/interrupted-results/README.md)
and [later memory stop](tests/agent-eval/proof-algorithms/interrupted-init-results/README.md)
remain separate; no closed model slot was rerun.

The [final CI gate](https://github.com/e6qu/fun-refactor/actions/runs/41466033975)
passed on the reviewed commit. Toolchain checks fetch only their installer inputs,
fixing Go's checkout timeout within its existing five-minute deadline.
The missing Mac runner check passed on a targeted retry, on the same platform.
Reviewed summaries and patches are public; raw transcripts remain private.

**Merged in PR #593: save checked proofs and deliver their edits together.**

The implementation and scripted controls now pass:

- `proof save` distinguishes `written` from `already-present`. A repeated save
  preserves source and history and needs no further write.
- `history patch` combines explicitly selected saved edits. Both GCD proofs reach
  one independently accepted patch. Conflicting file states, repeated IDs, missing
  snapshots and an empty combined result refuse. Git checks the patch and reverse.
- Changing a release number alone keeps existing measurements valid.
  Python and native versions must still agree.
- Guides explain the commands. The report correction preserves every original
  study outcome. The metadata comparison retains its earlier measurement and
  records the extra 265 response bytes on the same one-edit fixture.

All nineteen affected collection groups passed and their current records are retained.
The [final CI gate](https://github.com/e6qu/fun-refactor/actions/runs/41595629760)
passed all 92 jobs before merge. Those scripted checks establish tool behavior.

**Earlier result: [delivery of supplied GCD proofs](tests/agent-eval/proof-algorithms/local-results/M3-delivery/README.md).**
All twelve fixed slots ran once after fresh hosted qualification and six reported
passing local checks. Review found that the four OpenCode admission checks also
missed incomplete final usage. The stricter admission now rejects that gap.
Independent GitHub checking accepted all eleven delivered patches.
Five attempts met every completion requirement. Six completed OpenCode sessions
omitted their final response's token record from the JSON stream; one Kimi attempt
timed out without exporting a patch. Both proofs and the package had passed in
that timeout. Preserve all seven failures and the original 150-second limit.

Luna completed both repetitions with and without the finish card. OpenCode's
incomplete accounting prevents a reliable GLM/Kimi comparison. Do not adopt the
card as a measured improvement or start a transfer test. Both setups received
correct proofs; this comparison cannot close M3 or measure proof discovery.

The explicit OpenCode2 2.2.5 pin passes both local no-model reproductions and
[hosted qualification](https://github.com/e6qu/fun-refactor/actions/runs/41654594443).
All six scripted native exchanges, proof controls, Mac build and retained-patch
replay pass. The runner rejects missing or duplicate response records.

**Current result: [complete page-count comparison](tests/agent-eval/proof-argument-repair/README.md).**

All eight fresh GLM/Kimi attempts ran once after matching hosted and local
qualification. None submitted a patch. One Kimi draft passed fr checking near
135 seconds but was not saved or delivered. No mapped argument error was
observed, so the proposed repair mechanism remains unobserved.
[Independent grading](https://github.com/e6qu/fun-refactor/actions/runs/41918858328)
confirmed all eight failures. Keep the broader 36-attempt study paused.

The generator now uses explicit arithmetic imports and one compiler worker.
The collector enforces its existing ten-second finishing reserve inside the
150-second total. All eight complete attempts finished within that total.
The earlier interrupted collection and seven unstarted slots remain recorded
separately. Unknown usage and account charges remain unknown.

**Next work, together in the M3 milestone PR:**

1. Give each proof attempt its exact target, a valid initial cursor and current
   goal. Cover the recorded wrong-target, cursor and JSON-shape errors with
   runnable examples and regression checks. Use explicit settings and existing
   fr actions where they already supply the needed information.
2. Explain assumption structure and goal changes with checked Lean examples.
   For example, distinguish a pair of facts from a proof that implies another
   fact. Show how one closed branch differs from an accepted complete proof.
3. Check the complete save, package-build and patch-delivery workflow. Include
   a client that ignores its finishing deadline; its failed receipt and session
   export must still fit within 150 seconds and the existing resource caps.
4. Declare the next comparison before model calls. Measure the first useful
   proof check, observed repairs and independently accepted delivered patches.
   Compare any graph proposal with the same explicit command workflow. Require
   it to address a recorded error. Finish the named language cases,
   numeric proofs and source/model checks together; a familiar GCD delivery
   cannot replace them.

Do not repeat the full M3 study to seek a passing result. The
[team pilot](tests/agent-eval/team-solve/README.md) and unrun
[division-fact plan](tests/agent-eval/proof-division-facts/README.md) retain their
separate scope. Neither closes the solo proof-writing milestone.

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

