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
| OpenCode2 | `zai-coding-plan/glm-5.3-flash` | Explicit model and reasoning settings |
| OpenCode2 | `kimi-code-plan-global/k3` | The configured non-Pro Kimi K3; explicit reasoning settings |

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
M1 has its complete comparison. M2 is next; M3 follows it.

### M1. Run the selected agents on three complete tasks

The [results and every submitted patch](tests/agent-eval/selected-agents/local-results/README.md) compare a Boltons rename,
a Unicode comparison repair and a Lean proof of the fixed staff-or-owner access rule.
Three profiles, three tasks, two tool setups and two repetitions give thirty-six attempts.
The [task and client records](tests/agent-eval/selected-agents/README.md) own the exact requirements, versions and limits.

The installed local Codex and OpenCode2 CLIs ran serially with existing accounts and unchanged 150-second task limits.
GitHub checks submitted patches against the original source, including behavior or Lean checks, separate delivery and exact undo/redo.
Failures remain results; no failed slot was retried. A timed-out CLI stays failed even if it left a valid patch.
Only patches and summaries are public. Full CLI records and credentials stay local.
Account charges and unavailable usage remain unknown; a catalog's zero price is not observed billing.
The native CLIs expose no shared hard token/request cap. Their fixed time and resource limits remain visible.

The comparison does not establish general savings. Use the measured overhead and failed tasks to choose improvements in M2.
An unused fr tool establishes no benefit. A checked model proof leaves the source/model connection as a separate open claim.

### M2. Make multi-file refactorings and targeted changes dependable

Deliver this whole milestone in one PR. First freeze these four cases and the checks that reject wrong or incomplete changes:

| Case | Concrete requirement |
| --- | --- |
| Rust parameter removal | Remove an unused Boolean parameter with literal arguments across a two-crate workspace; update every declared direct caller and report external or unresolved uses |
| Regex feature | Add public `escape_len(pattern: &str) -> usize` in both regex crates; empty input needs zero bytes, `a.b` needs four and `é` needs two; construct no temporary escaped string |
| Python repair | Fix `invoice_total(amount, fee)` omitting the fee; `invoice_total(5, 2)` must return 7 |
| TypeScript rename | Rename an exported helper through an imported alias while leaving an unrelated local declaration with the old name untouched |

For every case:

- The agent finds the target from the request, reads the relevant code, previews one complete change and runs explicit project checks.
- Test preserved bytes outside declared edit ranges, comments, Unicode text and indentation.
  An intentionally wider rewrite must identify its wider range in the preview.
- Inspect affected imports, declared callers, tests and relevant configuration. Report omissions and unresolved uses.
- A refactoring must preserve argument evaluation. Test a side-effecting argument separately; preserve its effects explicitly or refuse before writing.
- A competing source edit makes the old preview refuse before writing. Refresh the review from current source.
- A failed check preserves its diagnostics and the actual edit state.
- Undo restores exact original bytes; redo restores the reviewed change.
  The exported patch passes independent checks in another checkout.
- Give each target agent one documented route for each task. Reuse existing edit/check machinery and remove redundant choices instead of adding another wrapper.
- Run two repetitions per case, profile and tool setup under frozen settings: forty-eight attempts.
  Preserve the earlier attempts and compare matching outcomes.

Do the implementation and deterministic checks before starting those attempts.
Start with Rust parameter removal and the TypeScript alias rename, then the regex feature and Python repair.
Use the M1 failures to shorten unnecessary reading and repeated tool work in the existing guides and commands.
Keep the 150-second task limit. Freeze the final instructions, client settings and independent checks before the first M2 call;
keep every outcome rather than tuning the experiment between attempts.

A literal argument uses a direct value, such as `false`.
A call such as `record()` must still run when its effects matter.
An imported alias is a local name for an exported function.
For `import { price as displayPrice }`, renaming the export updates `price` in the import while preserving calls through `displayPrice`.
The [pinned regex requirement](tests/agent-eval/unknown-target/task.json) also requires unchanged escaping and append behavior.
Test every escaping class, Unicode input, allocation counts and builds without default features in both crates.
Counting the required bytes allocates nothing. Producing nonempty escaped text allows at most one allocation; empty input allows none.

**M2 is complete when** all four cases pass the scripted delivery checks.
Each selected agent must also deliver at least one independently accepted result per case using fr within the frozen repetitions and limits.
Record which fr actions contributed to that delivered result.
Keep the failed outcomes alongside those successes; a scripted success alone cannot close the agent requirement.
Unsupported constructs must refuse until their required reusable rules work.
Add Python analysis or other language rules only when one of these code-change tasks needs them.

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

