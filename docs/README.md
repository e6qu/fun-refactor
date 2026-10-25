# Refactor code, make a change or write a Lean proof

Start with the [README](../README.md) to install the tool. The [plan](../PLAN.md) explains what
works today and the exact tasks planned for the selected smaller agents.
If a result is unclear, read [what you can conclude](understanding-results.md).

## Start with one of the three jobs

| Job | Complete first steps | Detailed guide |
| --- | --- | --- |
| Refactor while preserving behavior | [First checked rename](../TUTORIAL.md) | [Coordinated function changes](structural-changes.md) |
| Repair a bug or add a feature | [Write a targeted function change](body-authoring.md) | [Check the requested behavior](checked-outcomes.md) |
| Write or repair a Lean proof | [First Lean proof](first-lean-proof.md) | [Goal, tactics and checker workflow](agent-formalization.md) |

The [roadmap](../PLAN.md#next-work) names concrete refactoring, code-change and proof tasks.
It targets Codex with Luna Fast and OpenCode2 with GLM-5.3-Flash or non-Pro Kimi K3.
Those new live comparisons remain to be run.
Prefer preserving surrounding source. Keep refactoring behavior checks and deliberate behavior changes separate.

## Find supporting details

| What you want to do | Start here |
| --- | --- |
| Find the code behind a bug | [Tutorial](../TUTORIAL.md), then [finding and checking an explanation](agent-investigations.md) |
| Ask whether Python input reaches an output | [Python examples and limits](python-contract.md) |
| Follow an input through an original Python package | [Number and date questions](original-package-questions.md), then the [leap-year question](leap-year-package-question.md) |
| Ask another Python question using checked startup work | [Save preparation between questions](save-python-startup.md) |
| Compare the full cost of saving Python setup | [Optimized-build results](python-optimized-cost-results.md) and [measurement examples](measure-python-startup.md) |
| Understand why a Python import trace cannot finish | [Explain the recorded gaps](explain-python-imports.md) |
| Trace a Python call that replaces a stored default | [Default replacement example](replace-python-defaults.md) |
| Fix one function | [Change its implementation](body-authoring.md), then [test the requested behavior](checked-outcomes.md) |
| Rename a function or change its arguments | [Find affected callers](change-scopes.md) and [review the related changes](structural-changes.md) |
| Pick the next development task | [Concrete roadmap and completion checks](../PLAN.md#next-work) |
| Undo a mistake or continue interrupted work | [Recovery](host-recovery.md) and [correcting a failed edit](source-repair.md) |
| Move a route to another framework | [Route migration](feature-migration.md) |
| Check a stated property of a small function | [Proof support](lean-specs.md) |
| Check whether an explanation agrees with the compiler | [Compiler comparisons and limits](compiler-profile.md) |
| Use fr from an agent program | [Short agent guide](agent-skill.md) or [Python client](agent-runtime-sdk.md) |
| Decide whether fr saves work | [Product review](product-review.md) and [recorded results](evaluations.md) |
| Understand a performance number | [Comparisons, baselines and overhead](benchmarks.md) |
| Decide whether checking conditions would help Python analysis | [Numeric investigation and its comparisons](python-symbolic-investigation.md) |
| Contribute to fr | [Development](development.md) and [current handoff](continuity.md) |

The examples in the guides describe user tasks. Exact commands, data fields and limits remain
available below them. Historical experiment pages label their observations and unfinished work.

## Browse the references

Start with the [project README](../README.md) to install `fr` and run a first command. Use this page
to choose one authoritative guide for the task; detailed contracts link to related material instead
of repeating it.

For ongoing repository work, read the [plan](../PLAN.md) for priorities, the
[handoff](continuity.md) for the starting point and checks to preserve, and the
[development guide](development.md#working-on-a-shared-desktop) for workstation limits.

## Use `fr`

- [Tutorial](../TUTORIAL.md): complete a first inspected, reviewed and reversible change.
- [CLI reference](../CLI.md): every command, option and output contract.
- [Examples](../EXAMPLES.md): short task-oriented command sequences.
- [Recipes](../RECIPES.md): declarative multi-step refactoring.
- [Cross-language behavior](../CROSS_LANGUAGE.md): language resolution and edit rules.
- [IR reference](../IR.md): how supported code becomes a structured record for edits and translation.
- [HTTP contracts](../API_CONTRACTS.md): declared service-contract analysis.
- [Terminology](terminology.md): what report terms mean for your task.

## Integrate an agent

Begin with the quickstart, then choose the runtime or protocol detail needed by the integration.

- [Agent quickstart](agent-skill.md): install and use the portable skill.
- [Choose the next command](agent-workflow-guide.md): turn a structured task description into suggested steps.
- [Check the requested result](checked-outcomes.md): require a behavior test before delivering the patch or finishing the task.
- [Find affected code](change-scopes.md): list known callers and related tests, and see what the search missed.
- [Change a function and its callers](structural-changes.md): supported Rust renames and argument changes.
- [Compare two small models](model-comparisons.md): state the relation to check and resume its proof after verifying the inputs.
- [Python client](agent-runtime-sdk.md): send the agent the part of a report it needs.
- [Context workspace](agent-context-workspace.md): store reports and return the section the agent needs.
- [Reuse a report](agent-context-protocol.md): avoid repeated text while checking that the earlier report still applies.
- [Read only what you need](progressive-disclosure.md): open a function, relationship or source page when it answers the next question.
- [Check installed support](completion-audit.md): see which operations are available and what they can establish.
- [Evaluation evidence](evaluations.md): reproduce and interpret deterministic and live-agent runs.
- [Performance measurements](benchmarks.md): what each comparison measures, its baseline and the costs it includes or leaves unknown.
- [Codex runner](agent-codex-runner.md): run optional paid live-agent evaluations.
- [Unknown-target trials](unknown-target-investigations.md): compare discovery and resumed delivery on pinned upstream tasks.

For a change, check installed support, find and read the relevant code, then prepare a preview.
Review the whole diff, apply that reviewed change and run checks for the requested behavior.
Export the patch after those checks pass. The [goal guide](agent-workflow-guide.md) connects those steps;
the other pages explain each action and its limits.

## Understand and change code

- [Semantic model](semantic-model.md): inspect statements and expressions to prepare a precise edit.
- [Expression control](expression-control.md): ordered Python scalar flow, short-circuit calls and analysis boundaries.
- [Python call binding](call-binding.md): which argument reaches a helper, and which calls remain unsupported.
- [Literal defaults](literal-defaults.md): what happens when a caller omits an argument.
- [Scalar assignments](scalar-assignments.md): ordered unpacking, swaps and chained local writes.
- [Namespace packages](namespace-packages.md): static single-root imports and typed directory dependencies.
- [Body authoring](body-authoring.md): replace bodies and coordinate declarations.
- [Edit the item you inspected](disclosed-editing.md): use the returned edit reference to select a value or statement.
- [Cross-stack surfaces](cross-stack-surfaces.md): CSS, Tailwind, Markdown and Mermaid facts and edits.
- [Framework semantics](framework-semantics.md): React, Next.js, Express and FastAPI recognition.
- [Application IR](application-ir.md): describe supported routes and components before moving them.
- [Feature migration](feature-migration.md): reviewed migration and runtime evidence.

## Verify and recover

- [Project checks](project-checks.md): declare compiler, test and lint commands.
- [Compiler comparisons](compiler-profile.md): what the tool and compiler agree on, and what remains unchecked.
- [Lean support](lean-specs.md): check a stated property and see whether it connects to the actual code.
- [Agent formalization](agent-formalization.md): generate a project-specific proof workbench.
- [File transactions](file-transactions.md): review, apply and recover file changes.
- [Browser history](browser-history.md): bounded history in the WASM interface.

## Use Git

- Inspect [status](git-status.md), [changes](git-changes.md) and [diffs](git-diff.md).
- [Export and apply patches](git-patches.md).
- [Stage files](git-staging.md) and [undo or redo staging](git-stage-history.md).
- [Create reviewed commits](git-commit.md).
- Manage [worktrees](git-worktrees.md), including [creation](git-worktree-creation.md),
  [existing branches](git-worktree-existing-branches.md), [removal](git-worktree-removal.md),
  [resumption](git-worktree-removal-resumption.md), [recovery](git-worktree-recovery.md) and
  [archive compaction](git-worktree-archive-compaction.md).

## Maintain the project

- [Development](development.md): build, test, add languages and release.
- [CI latency and coverage](ci.md): test shards, complete coverage and runner time limits.
- [Roadmap](../PLAN.md): unfinished product outcomes and acceptance gates.
- [Retained results](roadmap-status.md): what earlier acceptance tests establish and which original claims remain open.
- [Earlier checklist](earlier-roadmap-checklist.md): the original A–D engineering items, outside the active milestone sequence.
- [Product review](product-review.md): efficiency criteria, scope and removal decisions.
- [Agent analysis review](agent-analysis-review.md): baseline evidence, analysis gaps and planning architecture.
- [Known defects](../BUGS.md): actionable defects and durable analysis boundaries.
- [Continuity](continuity.md): current contributor handoff.
- [Writing style](style.md): prose rules for user-facing text.
- [Changelog](../CHANGELOG.md): released changes.

Build and source records have their own focused guides:

- [Python package](../sdk/python/README.md): installation and client examples.
- [Query patterns](../queries/README.md), [compiled grammars](../grammars/README.md) and [upstream references](../vendor/README.md): recognize syntax and preserve its source records.
- [Browser playground](../web/README.md), [C interfaces](../wasm-shim/README.md) and [parser build support](../crates/tree-sitter-language/README.md): build the browser tool.
- [Translation examples](../tests/corpus/PROVENANCE.md) and [framework examples](../tests/framework-corpus/PROVENANCE.md): identify original test source and its limits.
- [Agent evidence sources](../tests/agent-eval/PROVENANCE.md), [Python project inputs](../tests/agent-eval/python-repository-flow/README.md) and [OpenCode project inputs](../tests/agent-eval/opencode/repositories/README.md): inspect which original projects a test used.

Historical design work belongs in Git history and retained evaluation fixtures. The active guides
describe the current product contract.

[Investigation evidence and resumable plans](agent-investigations.md) describes exact occurrences,
Python scalar flow, dependency invalidation and checked investigation delivery.

## Evaluate agent use

- [OpenCode tools](opencode-native-tools.md): how recorded attempts were run, limited and checked again.
- [OpenCode rehearsals](opencode-rehearsal.md): earlier small-task comparison protocol and retained results.
- [Source-grounded explanations](opencode-source-evidence.md): check answers against source the agent actually read.
- [Candidate review status](candidate-review-status.md): which proposed tasks met the study rules and which still need work.
- [Independent study](agent-study.md): compare whole tasks and count work by every agent, including failed attempts.
- [Evaluation evidence](evaluations.md): measured outcomes and historical limits.
- [Evidence restoration](../tests/agent-eval/EVIDENCE-ARCHIVES.md): recover archived results exactly.

Current priorities belong in the plan. Commands belong in the relevant guide. Detailed run histories
and immutable results belong with their evidence; do not copy them into the contributor handoff.
