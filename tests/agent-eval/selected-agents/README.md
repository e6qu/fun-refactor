# Compare the selected agents with and without fr

The question is concrete: does adding fr help the same coding agent deliver a correct change or proof?
The comparison is against that agent using its ordinary tools to read, search, edit and run commands.
Those ordinary tools remain available when fr is added.

Each of the three selected app/model combinations gets three requests:

- Rename Boltons' `clamp` function to `clamp_value`, update its callers and documentation, and preserve its behavior.
  Limiting 70 to the range 0 through 50 must still return 50.
- Fix the Unicode text comparison. Comparing `éa` with `éab` must return `2/3`.
  A Unicode character can occupy several bytes; this calculation counts adjacent character values rather than bytes.
- Write a Lean proof that the supplied access model returns true exactly when staff access or ownership is true.
  Lean checks proof steps against the fixed definition and claim. This does not by itself prove the original Rust program.

Three app/model combinations, three requests, two tool setups and two repetitions give **36 live attempts**.
The app, model, reasoning setting, source, requested result and limits stay the same within each comparison.
Tool order changes between repetitions.
These are familiar development tasks, not tests on previously unread projects.

## Check the experiment before making model calls

A scripted control supplies a known answer without asking a model to invent it.
It tests the experiment's machinery; it is not an agent success.

The task controls must accept a complete answer and reject unchanged or incomplete work.
They also check the delivered patch in another checkout, reverse it to recover the original files,
and reapply it to recover the reviewed change.
The native app controls use local scripted replies to check skill discovery, model settings,
tool execution, returned results and shutdown. Their containers have no external network access.

The GitHub workflow runs these checks. Follow the repository's
[workstation rules](../../../docs/development.md#working-on-a-shared-desktop) for local work.

## Read the inputs and results honestly

- [clients.json](clients.json) identifies the exact client downloads and checksums.
- [profiles.json](profiles.json) names the exact model, reasoning and speed settings.
- [tasks.json](tasks.json) fixes the requests, independent checks, proposed limits and repeat count.
- A proposed schedule marks all 36 attempts unstarted. Its usage and cost fields are unknown.
  It does not authorize model calls or establish live account access.

Correctness comes first. Compare time and cost for matching successful outcomes, and include failed attempts
when calculating the work spent per success.
Count setup, skill instructions, fr indexing, tool calls, checks, retries and shutdown.
An offered but unused fr command establishes no benefit.
Unavailable billing information stays unknown; a model catalog's zero price is not proof that a run is free.

[M1 in PLAN.md](../../../PLAN.md#m1-run-the-selected-agents-on-three-complete-tasks) owns completion.
Its implementation, all required live attempts, results and tracking belong in one PR.
