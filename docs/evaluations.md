# What our tests and experiments tell us

Use this page to find the evidence behind a claim about fr.
A scripted repair tests a known sequence of edits. A live comparison asks whether an agent benefits while discovering a solution.
For example, fewer reply bytes cannot establish a cheaper complete bug fix.

## Start with the current question

| Your question | Current source | What remains unknown |
| --- | --- | --- |
| What has been checked, and what comes next? | [Retained results](roadmap-status.md) and [active roadmap](../PLAN.md) | Earlier qualification counts do not complete the new milestones |
| Which Python behavior can I use? | [Python examples and limits](python-contract.md) and [original-package questions](original-package-questions.md) | Unsupported behavior and unfinished whole-package imports |
| How much overhead does calling fr through Python add? | [Same-question CLI and Python comparison](project-session-cost.md) | Whether an agent completes unfamiliar tasks more cheaply |
| Does saving work help after edits? | [Saved-work results](saved-work-cost-results.md) and [follow-up](saved-answer-improvement.md) | General savings beyond the tested projects and questions |
| Does fr improve whole agent tasks? | [Selected-agent baseline](../tests/agent-eval/selected-agents/local-results/README.md) and [four code changes](../tests/agent-eval/project-edits/local-results/README.md) | General success rates and savings beyond these tasks; fr added time on matching successes |
| Does a proof cover the code users run? | [Proof guide](lean-specs.md) and [proof-writing milestone](../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs) | Model proofs and source-implementation claims have separate completion conditions |

The [measurement guide](benchmarks.md) explains each baseline, included work, units and missing costs.
It separates generated-program runtime, tool runtime, text sent to agents and complete-task cost.

## Evidence classes

A fixture is a chosen test input. An evaluator runs a test procedure.
An oracle supplies an independent expected answer, such as ordinary Python's result for the same input.
An attempt records one run. An arm is a compared tool setup.
A manifest identifies the exact files and settings behind a record.

| Evidence | What it can establish | Example limit |
| --- | --- | --- |
| Scripted workflow | A known change can be applied, checked, reversed and transferred | The script already knows the repair |
| Live agent attempt | This agent produced this outcome with these settings | One attempt cannot establish a general success rate |
| Paired comparison | Two setups attempted the same task with the declared controls | Missing retries or setup costs make a cost claim incomplete |
| Model proof | A rule holds under stated definitions and assumptions | A separate model can describe different behavior from the shipped code |
| Compiler or behavior check | The chosen commands and tested inputs passed | Untested inputs remain unchecked |

For “does fr make repairs cheaper?”, compare the same model and task with ordinary tools and with those tools plus fr.
Count instructions, setup, indexing, retries, checks, every helper agent and failed attempts.
Preserve missing billing as unknown. Do not remove unsuccessful attempts to improve the average.

## Current records and historical observations

[The acceptance catalog](../tests/agent-eval/roadmap.json) retains the earlier completion requirements.
The [focused roadmap](../PLAN.md) owns the active M1, M2 and M3 tasks.
[The scalar-report map](../tests/agent-eval/current-scalar-reports.json) selects active Python and workflow records.
[The status report](roadmap-status.md) checks their recorded inputs and states what those records establish.
Installed support comes from `fr audit`, `fr capabilities` and command schemas.

Raw run records live under `tests/agent-eval/results/`. A manifest connects source files, checking programs, binary and outputs through content fingerprints.
Changed inputs require fresh collection. Preserve the earlier records and their failures; do not rewrite a hash to make an old result appear current.

The [detailed history through PR #577](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md) retains the earlier tables, commands, original results and failed attempts.
The subject index below explains what each line of work tested and links its current guide.
Following an old record does not restart its study or authorize paid calls.

## Retained Boolean model comparisons

Three true-or-false inputs give eight combinations. Model proofs and Rust/Python executions checked these cases, including a rejected argument mapping. They did not prove arbitrary source translations.

[Current guide](model-comparisons.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#retained-boolean-model-comparisons)

## Consumer scope and delivery

A selected Python caller chain checks finding affected calls, repairing subtotal and replaying the patch. Broader affected-code discovery remains open.

The [current caller-chain record](../tests/agent-eval/results/2026-10-26-m2-current-change-scope/manifest.json) identifies the exact source, checks and exported patches.

[Current guide](change-scopes.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#consumer-scope-and-delivery)

## Checked delivery outcomes

A known repair must pass its declared behavior checks before delivery. A passing build alone cannot close the reported bug.

[Current guide](checked-outcomes.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#checked-delivery-outcomes)

## Unknown-target investigations

The earlier two-task Luna experiment compared ordinary tools with ordinary tools plus fr. The Unicode repair passed in both attempts; the allocation-constrained feature passed only with fr. Each pair ran once. These are observations, not general success rates.

[Current guide](unknown-target-investigations.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#unknown-target-investigations)

## Package investigation and delivery

Importing an unchanged package matters: a copied helper skips the package startup that may change its behavior. Separate repair tests check application, reversal and receiver checkouts.

[Current guide](original-package-questions.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#package-investigation-and-delivery)

## Current matched results

Use the measurement guide for baselines and added costs. Direct CLI calls, restarted Python clients, running clients and saved answers perform different preparation. Compare the same questions and correct results before comparing their times.

[Current guide](benchmarks.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#current-matched-results)

## Find the underlying evidence

The current scalar-report map and acceptance catalog identify the active records. Their manifests identify exact source, procedure and output fingerprints. Restore archived files before auditing a historical run.

[Current guide](../tests/agent-eval/EVIDENCE-ARCHIVES.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#find-the-underlying-evidence)

## Reproduce and audit

Use the hosted refresh groups for fresh measurements and the recorded evaluator for a historical audit. Full builds and report collections run on GitHub. Paid trials need agreed models, prices, caps and complete accounting before they start.

[Current guide](ci.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#reproduce-and-audit)

## Deterministic investigation acceptance

A scripted investigation can check finding a location and testing a supplied repair. It cannot show whether an independent agent would discover that repair.

[Current guide](agent-investigations.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#deterministic-investigation-acceptance)

## Fixed-point and reuse acceptance

Repeating analysis until it adds no more facts can finish a supported value-flow calculation. Reusing its saved answer must still agree with fresh analysis after relevant edits.

[Current guide](agent-investigations.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#fixed-point-and-reuse-acceptance)

## Semantic provenance and checked plans

A finding needs its source locations and assumptions. Saving a task plan preserves observations; it cannot renew permission to apply an outdated preview.

[Current guide](semantic-model.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#semantic-provenance-and-checked-plans)

## Recursive scalar flow

For helper calls that call each other, the analysis must track which argument contributes to the answer and keep unfinished calculations explicit.

[Current guide](expression-control.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#recursive-scalar-flow)

## Bounded flow fact acceptance

An exhausted work limit leaves the question unfinished. A report with no connection cannot then establish that input never reaches output.

[Current guide](understanding-results.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#bounded-flow-fact-acceptance)

## Compiler evidence acceptance

Retain the compiler version, options and source behind a comparison. Agreement on those cases does not establish that fr models the whole language.

[Current guide](compiler-profile.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#compiler-evidence-acceptance)

## Durable target resumption

A function may move or change after a task is saved. Reopen it with a checked current reference and report missing or ambiguous matches.

[Current guide](agent-investigations.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#durable-target-resumption)

## Retained proof evidence

Saving proof output is useful only while its inputs and checked assumptions still apply. A model theorem needs a separate connection to the shipped implementation.

[Current guide](lean-specs.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#retained-proof-evidence)

## Imported scalar flow

A trace through an imported helper must include the chosen import and startup behavior. A same-named local file can change which helper is called.

[Current guide](python-contract.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#imported-scalar-flow)

## Repeated consumer discovery

A later edit can add a caller or change the affected files. Rediscover before claiming an earlier change scope is current.

[Current guide](change-scopes.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#repeated-consumer-discovery)

## Fresh resolution

Name resolution connects a written use to its declaration. A nearer local binding can hide an imported function; the two uses must stay distinct.

[Current guide](compiler-profile.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#fresh-resolution)

## Native host recovery

An interrupted write can leave a transaction needing recovery. Inspect its saved stage before retrying, then verify restored files and declared checks.

[Current guide](host-recovery.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#native-host-recovery)

## Scoped structural delivery

A rename or argument change must account for its supported callers and refuse unsafe cases. Export and test the reviewed patch in another checkout.

[Current guide](structural-changes.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#scoped-structural-delivery)

## Expression control and checked delivery

Evaluation order matters. An unchecked operation after a displayed location still prevents a complete answer, even if the reported locations look useful.

[Current guide](expression-control.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#expression-control-and-checked-delivery)

## Required call binding and checked delivery

A helper's parameter is a named argument slot. Positional and named arguments must reach the right slots before tracing its result.

[Current guide](call-binding.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#required-call-binding-and-checked-delivery)

## Literal defaults and exact signatures

Omitting an argument can select a declaration's default value. That differs from explicitly passing a value, and signature edits must preserve the distinction.

[Current guide](literal-defaults.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#literal-defaults-and-exact-signatures)

## Single-root namespace packages

A Python import can use a directory without an ordinary package initializer. Retain the searched directories and competing candidates so import edits invalidate affected answers.

[Current guide](namespace-packages.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#single-root-namespace-packages)

## Ordered scalar assignments

A swap reads the old values before assigning the new ones. Chained assignments and unpacking must preserve their actual order.

[Current guide](scalar-assignments.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#ordered-scalar-assignments)

## Roadmap outcomes on pinned Python repositories

A small supported program and a full original import answer different questions. The current status report keeps original-package successes and remaining Toolz, Funcy and Pydash obstacles separate.

[Current guide](roadmap-status.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#roadmap-outcomes-on-pinned-python-repositories)

## Python development history

Earlier development examples introduced loops, helper calls, startup tables, saved startup, supported collections and explicit error handlers. Their original observations remain in the dated history; the Python contract owns current support.

[Current guide](python-contract.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#python-development-history)

## Replace a stored default with a supplied value

A supplied value can replace a default kept during startup. A replacement must remove the earlier value's connection when Python would do so.

[Current guide](replace-python-defaults.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#replace-a-stored-default-with-a-supplied-value)

## Check a small function that fills a table during import

Importing a module can populate a callback table before a question starts. The startup observation is evidence of one execution, not a proof about every execution.

[Current guide](python-package-startup.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#check-a-small-function-that-fills-a-table-during-import)

## Ask several Python questions after checking startup once

Preparing supported startup once can serve several questions. Each saved answer must retain the files, settings and checker inputs needed to detect changes.

[Current guide](save-python-startup.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#ask-several-python-questions-after-checking-startup-once)

## Finish three original-package questions

Toolz's return_none, Funcy's inc and dec, and Pydash's add remain named whole-package questions. Do not replace them with isolated copies or count unfinished imports as negative answers.

[Current guide](explain-python-imports.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#finish-three-original-package-questions)

## Pass dictionaries to a startup function

Dictionary keys, values and later writes can affect a helper selected during import. Supported examples need both reaching-input and overwritten-value cases.

[Current guide](original-package-questions.md) · [Original observations](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/evaluations.md#pass-dictionaries-to-a-startup-function)
