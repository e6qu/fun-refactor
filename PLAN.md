# Finish the right change and show that it works

Give an agent a bug report in a project it has never seen.
Help it find the relevant code, deliver a working patch, and continue safely after another edit.
Measure the whole attempt, including wrong turns, checks and setup.

## What works today

`fr` finds functions and known callers, previews edits, runs declared checks, and saves changes for undo or patch delivery.
Selected Rust and Python repairs pass independent behavior tests and replay in another checkout.
Python examples cover original package imports, simple values, helper calls, cleanup and selected explicit error handlers.
Claude Code, Codex and OpenCode can discover the project skill and complete scripted exercises.

We have not demonstrated general agent savings.
The [current results](docs/roadmap-status.md) own test counts and completion claims.
The [cost comparison](docs/project-session-cost.md) compares ways of calling the same compiled fr program, including Python client overhead.
It does not measure an agent discovering a repair.

## Next work

Each row names a delivery package. Keep its implementation, worked example, checks and tracking together in a substantial PR.
R1 is next. R2 follows it. R3 can proceed once its source and arithmetic rules are fixed.
R4 needs agreed models and spending limits before any paid attempt.

| Work | User outcome | State | Completion check |
| --- | --- | --- | --- |
| [R1. Recover from invalid number input](#r1-recover-from-invalid-number-input) | A bounds form reports bad input and preserves unexpected errors | Next | Conversion, handler and error-message cases agree with Python; checked patch replays |
| [R2. Follow shared lists through original imports](#r2-follow-shared-lists-through-original-imports) | A helper changing a shared list changes the caller's answer too | Planned | Alias and separate-list cases pass; full-package checks and an independent first attempt qualify |
| [R3. Prove the properties of the code we ship](#r3-prove-the-properties-of-the-code-we-ship) | A page count and an edit-range check satisfy their stated rules | Planned | Proofs cover the actual implementations; changed or mismatched code cannot keep a passing claim |
| [R4. Measure agents completing whole tasks](#r4-measure-agents-completing-whole-tasks) | Learn whether adding fr improves correct repairs and total cost | Waiting for live settings | Complete prepared comparison, then the separate repeated-task study; retain every outcome and cost |

### R1. Recover from invalid number input

Extend the [Boltons-backed bounds application](docs/catch-python-errors.md).
This is our authored application using an unchanged library, not a newly discovered upstream bug.
It is a familiar development example.

The application currently follows an explicit `raise ValueError(...)` from Boltons.
The next change accepts decimal number text from a form and handles errors raised by the conversion itself.
For example, `int("twelve", 10)` raises `ValueError` without a written `raise` statement.

Start by committing the application requirements and independent Python checks.
Use the original pinned Boltons import. Then implement the general rules needed by those requirements.

Initial language scope:

- Base-ten `int(text, 10)` for ordinary strings, including signed values, valid whitespace and malformed input.
- Matching `ValueError` recovery while an unexpected `TypeError` still leaves the application.
- `except ValueError as error` and `str(error)` for the supported builtin errors.
  The binding names the caught error object; its lifetime ends after that handler.
- Error classes and their input connections remain separate through helpers, nested cleanup and reraising.
- Unknown conversions, custom objects and other unsupported error behavior keep the answer incomplete.

Completion requires all of these:

1. Valid numeric text, malformed text, reversed bounds and a wrong-type argument pass independent behavior checks.
2. A caught error message can retain its connection to submitted text; a replacement message removes that connection.
3. Reading the caught name after its handler cannot reuse a value Python has deleted.
4. Each rule passes independently authored programs, renamed functions and moved files.
5. The repair uses fr preview, application, declared checks, undo, redo and patch replay in another checkout.
6. Relevant edits expire saved answers; saved and fresh checks agree. Existing work, response and time limits stay unchanged.
7. The tutorial supplies files, commands, expected results and the remaining unsupported behavior.

Passing this package adds supported behavior. It does not establish that every Python operation is free of runtime errors.

### R2. Follow shared lists through original imports

Extend the bounds application with a list of validation messages passed to a helper.
Two names can point to the same list. A helper changing that list must affect later reads through either name.

Start with this distinction:

```python
messages = [customer_label]
shared = messages
shared[0] = "PUBLIC"
send(messages[0])  # The output is PUBLIC.
```

A separate list, such as `separate = [messages[0]]`, holds its own slots.
Changing `separate[0]` must preserve the original `messages[0]`.

Initial scope is ordinary list creation, assignment of another name, constant-index reads and writes, `append`, and `clear`.
Keep arbitrary object attributes, overloaded operations and unsupported collection behavior explicit.

Completion requires:

1. Same-list and separate-list cases agree with ordinary Python.
2. Writes made by a helper appear in the caller's later reads. Overwriting a slot removes its old value connection.
3. Edits to the writing helper expire affected saved answers; unrelated answers remain usable.
4. The application repair passes behavior checks, undo and independent patch replay through its original imports.
5. Recheck the existing full-package questions for Toolz's `return_none`, Funcy's `inc` and `dec`, and Pydash's `add`.
   Record each import/startup obstacle and fix the required reusable rules. Copies of isolated helpers do not meet this check.
6. Reserve one new project before inspecting its implementation, using a question from its public documentation.
   Record its revision, inputs, expected results and limits. Freeze the checker before its first implementation read.
7. Use one unchanged checker for the qualifying project questions. Preserve a failed first attempt if one occurs.
   A later fix makes that project familiar; another independent first attempt needs another reservation.

Broader caller discovery belongs to the same repair: inspect affected code, configuration, documentation and tests before delivery.

### R3. Prove the properties of the code we ship

A passing test checks chosen inputs. A proof checks a stated rule under explicit assumptions.
The missing connection is showing that the proof describes the implementation users run.

Use these two tasks:

- Existing code: [the page-count helper](tests/agent-eval/checked-translation/pages.rs), with its [current contract](tests/agent-eval/checked-translation/contract.json).
  Eleven items at five per page need three pages. Invalid sizes return `-1`; large valid inputs must avoid integer overflow.
- New code: an edit-range helper `fits_window(offset, count, total)` with explicit unsigned 64-bit inputs.
  These are whole numbers from zero through 18,446,744,073,709,551,615.
  An empty range at the end is valid. A range past the end is invalid.
  Use `offset <= total && count <= total - offset` so the check avoids addition overflow.

Completion requires a checked source-to-model connection for both functions, their specifications and Lean proofs.
Record source files, arithmetic rules, translation rules, tool versions and every trusted component.

Check both saved and newly attempted proof claims after deliberate mistakes:
change a comparison, return the wrong result, alter the model, or change a relevant dependency.
Outdated claims must expire; a fresh proof attempt must also reject the wrong implementation.
Keep behavior checks and independent patch replay passing.
A theorem about a separate description cannot satisfy this package.

Use existing symbolic execution tools when a concrete check needs help finding a failing input.
Symbolic execution explores conditions with unknown inputs.
The [earlier experiment](docs/python-symbolic-investigation.md) found no additional failing behavior beyond its generated tests.

### R4. Measure agents completing whole tasks

The [prepared first comparison](docs/agent-comparison-preparation.md) has two tasks:
repair Unicode string comparison, and calculate escaped text length without allocating a temporary string.
Use Claude Code, Codex and OpenCode with ordinary tools, those tools plus CodeGraph, and those tools plus fr: eighteen first attempts.

Before live calls, agree and record exact models, reasoning settings, credentials, prices, spending caps and full-attempt resource limits.
Changing a task, tool, skill or checking program requires reviewing the prepared schedule again.
Do not restart the stopped studies.

For every attempt, record correct behavior and regressions, offered tools actually used, retries, setup, indexing, checks and shutdown.
Count model usage, total cost, elapsed time, processor work, memory and storage, including every child program and failed attempt.
Report missing billing data as unknown. Count first-use setup separately from later reuse.

Then meet the distinct [48-attempt repeated-task requirement](docs/product-review.md#repeated-task-study).
It uses four task types across at least three independent projects, two model families and the stated repetitions and delegation limits.
The eighteen-attempt comparison cannot close that requirement.

Success means complete, independently checked results and honest costs.
Slower, more expensive or inconclusive outcomes remain valid results.

## Milestones and implementation checklist

The task packages above say what to do next. These checkboxes retain the established completion requirements.
The [acceptance catalog](tests/agent-eval/roadmap.json) and [status report](docs/roadmap-status.md) keep the detailed checks and recorded results.

### A. Find relevant code and explain the evidence

Open: independent agents completing unfamiliar tasks with complete cost records. R4 addresses it.

- [x] Report exact declaration, reference and call locations.
- [x] Link analyzed expressions to source and expose missing or ambiguous locations.
- [x] Explain findings with their inputs, assumptions and omitted information.
- [x] Retain compiler inputs, versions and disagreements with fr.
- [x] Save requested outcomes, checked facts and remaining questions.
- [x] Run the initial bug and feature comparison without supplying edit locations.

### B. Explain how values move through code

Open: reusable rules for the remaining Python behavior and original-package examples. R1 and R2 address it.

- [ ] Specify reusable language rules connecting source, analysis and edits, with independent examples and explicit limits.
- [x] Represent supported assignments, branches, loops, returns and explicit exceptions.
- [x] Describe supported argument passing, returns and actions, including recursive calls.
- [x] Trace user-declared inputs, outputs and cleaning functions with source locations.
- [x] Distinguish possible calls and value paths from statements established by a proof.

### C. Resume work and reuse correct answers

Complete for its stated checks. Preserve these results in every task package; agent savings remain a separate question.

- [x] Distinguish current code references, saved-content fingerprints and candidate matches after edits.
- [x] Track source, import searches, settings and analyzer inputs behind answers.
- [x] Verify saved answers and analyze again when reuse cannot be established.
- [x] Resume saved tasks after discarding outdated findings.
- [x] Invalidate check and proof results after relevant input changes.
- [x] Measure first use, reuse and rechecking after edits on three independent projects.

### D. Make checked changes and useful proofs

Open: complete affected-code discovery and proofs connected to existing and new implementations. R2 and R3 address them.

- [ ] Discover all relevant callers, contracts, configuration and tests for the declared change.
- [x] Add general language rules needed by observed structural changes.
- [x] Translate a function under explicit arithmetic, error and evaluation-order rules.
- [ ] Connect proofs to an existing implementation and to a new implementation developed with its specification.
- [ ] Retain proof inputs for edits and translations, rejecting outdated claims.
- [x] Check small models governing edits, task transitions and interrupted-write recovery.

## Generalization requirements

- Write the behavior requirements before implementing support. Compare positive, negative and unsupported cases with independent execution.
- Renaming a function or moving its file must preserve the answer.
- Keep production behavior independent of repository names, revisions and expected patches.
- An incomplete answer cannot establish that an input never reaches an output.
- Preserve first unfamiliar-project attempts and all failures, timeouts and unstarted work.
- Measure whole task outcomes. Shorter reports or scripted repairs alone cannot establish agent savings.

## Working rules

Use `fr --json audit` and `fr capabilities` for installed support.
Use fr on its own code where suitable, and record concrete gaps.
Keep settings explicit, with one current interface and format. Add no compatibility readers, fallback routes or needless wrappers.

Builds, full tests and large collections run on GitHub. Follow the [workstation limits](docs/development.md#working-on-a-shared-desktop).
Fix failures and timeout causes without increasing limits or dropping coverage.
Refresh reports after their recorded inputs change; keep the original records and hashes intact.

Use the [handoff](docs/continuity.md) for current maintenance instructions, [bug list](BUGS.md) for reported defects,
and [documentation index](docs/README.md) for user and developer guides.

