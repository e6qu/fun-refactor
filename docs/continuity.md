# Pick up the current work

Read the [plan](../PLAN.md) for the goal and [current results](roadmap-status.md) for the checked requirements.
This handoff tells the next contributor what to do now. Earlier failures belong in the
[development history](evaluations.md#python-development-history).

## What users can do now

An agent can find functions and callers, read selected code, prepare a reviewed edit, run declared
checks, and keep changes for undo or resumption. Small Python examples can also follow an input
through a helper to an output.

For example, you ask whether a customer-supplied value reaches `send(value)`. A supported helper
can return a trace with its source locations. If importing the helper runs unsupported setup code,
the result stays incomplete. An empty trace then leaves your question open.

Toolz, funcy and pydash still give incomplete full-package answers. Some copied helpers work in small
isolated files. That helps test a language rule; it does not establish that the original package works.
We also have not demonstrated that fr saves time or money on unfamiliar tasks.

## What to work on next

Use the reported import and startup statements to choose the next Python language rule.
The [Python guide](python-contract.md) owns current support and limits; avoid maintaining another
support list here.

| Observed problem | Why it matters to a user | Next investigation |
| --- | --- | --- |
| Toolz builds and changes a signature registry | Import setup can affect which helper the application calls | Inspect the remaining registry operations and their effects |
| Funcy builds `FACTORY_REPLACE` using runtime calls before its `key=EMPTY` definitions | Unchecked earlier setup clears the known names available to later defaults | Investigate that dictionary's type and method calls, then revisit object-marker calls |
| Pydash exhausts 128 import lookups across distinct imported names | The search stops before it can answer the selected trace question | Investigate bounded answers for the selected helper while preserving import-time effects and the existing limit |

Full-package answers remain incomplete. A smaller list of startup blockers is progress toward an
answer, but it is not a successful package analysis. The [library results](python-repository-flow.md)
retain the complete attempts and limits.

Keep jaraco.functools unread during development. Freeze the implementation before its first evaluation.
Toolz, funcy and pydash are familiar development examples. The
[selection record](../tests/agent-eval/python-repository-flow/replacement-holdout.json) explains the reserved project.

Keep editing, undo, resumption and patch delivery working as the analysis expands.
After useful real-project answers improve, compare complete unfamiliar tasks and count all reading,
retries and failures. Do not infer savings from a smaller report or a passing scripted repair.

## Keep the documentation useful

Both broad documentation passes are merged. Keep detailed sections, examples and cross-links current.
Start with the user's question, show the action, explain the result and say what still needs checking.
Use the [writing guide](style.md) and [result meanings](understanding-results.md).

For example, “the build passed” still needs the test that reproduces checkout's rounding error.
A saved review becomes stale when another edit changes the files it covers.

## Keep saved results trustworthy

Keep one current analyzer, SDK and report format. Missing fields and older formats fail explicitly.
Historical results use their archived original code.

When code or a recorded input changes, collect the affected results on GitHub.
Keep their file fingerprints and run identities. A wording edit to `docs/python-contract.md` also changes
a recorded input. Finish such edits before collection; never edit a hash to make stale evidence pass.

The [current collection receipt](../tests/agent-eval/results/2026-10-15-type-registries-scalar-collection/manifest.json)
identifies the active reports. Preserve failed attempts beside corrected ones; the
[development history](evaluations.md#python-development-history) keeps their explanations and links.

## Studies that stay stopped

The [client investigation](../tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md) is closed.
The [packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md) stays stopped.
Keep its failed and unstarted attempts. A new live comparison needs independent tasks, agreed resource
and cost limits, and complete usage records. The [candidate review](candidate-review-status.md) lists remaining gaps.

## Before editing or merging

- Follow the [workstation rules](development.md#working-on-a-shared-desktop). Use the local guard; full builds, full gates and large collections run on GitHub.
- If the guard refuses, move the work to CI. Keep its limits.
- Fix the cause of a failure or timeout. Keep the existing deadline.
- Check recorded input fingerprints before describing a result as current.
- Use the configured GitHub login; leave credential files alone.
- Keep each commit message to one line of at most 80 characters, with no body or trailers.
- Wait for passing CI before a merge. When merging is authorized, squash with an explicit subject and an empty body.
