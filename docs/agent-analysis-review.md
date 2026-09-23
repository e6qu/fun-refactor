# Agent analysis and planning review

Review date: 2026-09-23. Baseline: `c22ba827`, including merged PR #355.
The [roadmap](../PLAN.md) owns priorities and acceptance gates. This review records the reasoning
and implementation evidence behind that direction. Proposed contracts below describe future work.

## Assessment

`fr` has a substantial foundation for agent discovery, constrained authoring and reviewed delivery.
Its strongest property is evidence discipline: bounded reports, explicit uncertainty, exact review
bases and distinct verification claims. Preserve that property as analysis grows.

The representative guided delivery milestone is complete for its pinned corpus. General agent
investigation remains an open product outcome. Current acceptance establishes specific read,
change, migration and proof routes. It does not establish broad bug diagnosis, interprocedural
dataflow, security analysis or durable planning across changing revisions.

The next investment should connect analysis to task decisions. An agent needs to locate relevant
code, explain a relationship, identify missing evidence, choose a change and verify the result.
Adding another wrapper around an existing edit contributes less until an observed task needs it.

## What exists

| Area | Current evidence | Boundary |
|---|---|---|
| Parsing and indexing | Nineteen parser identities; declarations, scopes, references, imports and receiver evidence | Parser support does not establish complete semantic analysis |
| Code map | Directory and lexical containment, relationships, packages, entry points, framework facts and test associations | The map explicitly says architecture is not inferred |
| Exact source navigation | Revision-bound declaration handles, name and definition locations, typed Python targets and verified source fragments | Occurrence locations and semantic-node origins need a common public contract |
| Progressive discovery | Semantic, evidence and project views; bounded pages, continuations, omissions and exact follow actions | A bounded answer may leave relevant code unexplored |
| Merkle storage | Canonical object records, subtree deduplication, optional inclusion proofs and verified local or HTTP stores | Content identity alone establishes neither semantic truth nor applicability to another revision |
| Calls and impact | Incoming/outgoing traces, dispatch candidates, cycles, confidence and bounded caller impact | Candidate calls do not establish runtime dispatch or value transfer |
| Value flow | Local initializer origins and reference uses, plus configuration provenance and stitching | Function boundaries stop imperative value tracing; security semantics need additional models |
| Agent routing | Structured purposes, deterministic guides, native intent packets and typed SDK operations | Route selection does not manage a continuing investigation |
| Changes | Refactors, recipes, semantic edits, authored bodies, surface edits and application migrations | Support varies by language, construct and effects |
| Delivery | Immutable reviews, checks, recoverable history, reversal and Git patch delivery | Behavioral correctness requires suitable oracles |
| Lean | Anchors, signature maps, model obligations, proof authoring, executable kernels and finite correspondence cases | General source implementation equivalence remains an explicit obligation |
| Evaluations | Pinned deterministic cases and accepted guided live trials with independent oracles | Task breadth, repeated trials and unknown-target investigations need expansion |

The live audit is the authority for support counts. Its capability cells are admission predicates,
not accuracy measurements. The [evaluation guide](evaluations.md) identifies accepted manifests
and separates deterministic evidence from live trials.

## Gaps that affect agent work

### Exact occurrences and analysis provenance

Declaration locations now connect AST identities to editable text. `src/span.rs` defines byte spans
and line ranges; `src/project.rs` exposes them in targeted declaration reports. Python verifies
the selected targets and revealed text.

Relationship endpoints still expose a line, and call sites expose a starting position.
`src/project/evidence.rs` also emits starting positions for flow and impact evidence. These reports
need occurrence ranges for the specific reference or expression, distinct from the owning declaration.
An agent investigating two calls on one line needs both identities.

Adopt one reusable occurrence record: snapshot, path, exact span, derived line range, role and
enclosing declaration when available. Keep the existing Unicode column and EOF conventions explicit.
Semantic nodes need origin mappings with explicit absent or multiple origins for generated or
combined constructs. Avoid inventing a source span for a synthetic node.

Analysis facts also need their rule, inputs, confidence, scope and omissions. Allow the agent to
ask why an edge exists and which assumption limits it. Expose full locations and derivations through
bounded detail actions so overview packets stay small.

Build configuration belongs in that evidence scope. Record the selected feature flags, generated
inputs and dependency universe when they affect an answer. Keep inferred test associations separate
from observed test coverage. Optional compiler facts can improve resolution where a pinned task
requires them; retain their toolchain identity, provenance and disagreements with syntax evidence.
Ordinary structural discovery should retain its current configuration-free path.

### Control flow and value flow

`src/analysis/flow.rs` walks initializer dependencies and uses. `StopReason::CrossesFunctionBoundary`
explicitly stops at parameters and resolved call results. The evidence view names its sources and
sinks as local origins and uses. These are useful foundations for investigation.

The next layer needs an analysis IR with explicit control-flow edges and value definitions.
Begin with supported functions in one language. Model assignment order, branches, loops, returns
and exceptional exits where admitted. Report unsupported effects at the affected boundary.

Then add bounded function summaries for argument-to-parameter and return-to-result transfer.
Declare context sensitivity, recursion handling, alias assumptions and termination budgets.
External calls require explicit summaries or unknown effects. A call graph path alone cannot
answer whether a particular value reaches a use.

State whether an analysis overapproximates possible behavior, proves a condition, or offers a
heuristic candidate. Confidence labels alone cannot establish a soundness guarantee. Prioritize
correctness and measured precision in a declared subset before widening language coverage.

Security queries need versioned source, sink, propagation and sanitizer rules. A sanitizer has
specific input, output and context semantics. A matching function name cannot establish safety.
Return a witness with exact occurrences, assumptions, rule identities and cutoffs. Distinguish
potential paths from feasible paths. An exhausted search cannot establish absence.

### Task planning

`src/project/agent_guide.rs` selects a route from a structured goal. `src/project/task.rs` combines
bounded requests, authored targets, checks and delivery. Those are useful primitives for a task plan.

A continuing task also needs acceptance criteria, hypotheses, evidence dependencies, unresolved
questions and progress. Add a typed task plan above the existing guide and review lifecycle.
Keep natural-language interpretation and proposed hypotheses with the agent. Let `fr` validate
references, prerequisites, budgets, transitions and completion evidence.

A proposed task step contains:

- A question or intended outcome, with a task-local identity.
- Dependencies and required facts, each tied to a snapshot and analysis version.
- A bounded discovery action, authored change, check or proof obligation.
- Explicit states: pending, ready, running, satisfied, blocked or stale.
- Evidence that satisfies the step, plus remaining uncertainty and acceptance criteria.

Hypotheses remain distinct from tool facts. A successful command cannot by itself satisfy an
acceptance criterion. Mutation steps continue through complete immutable reviews. Investigation
steps can refine the plan without granting permission to execute a changed preview.

Start with a local serializable plan. Test interruption, reopening and revision changes before
adding scheduling or concurrent execution. Existing history remains the authority for mutations.

### Merkle reuse across revisions

The current Merkle implementation already addresses disclosure and cache integrity.
`docs/progressive-disclosure.md` specifies both tree commitments and deduplicated object records.
`src/cache.rs` also caches extracted facts, resolution snapshots and analyses.
`src/index.rs` derives resolution keys from the workspace fact set.

Incremental analysis requires dependency records in addition to content hashes. An unchanged
function can acquire a different meaning when imports, build options or a called function change.
Record dependencies on those inputs, including negative lookups and external summary versions.

Keep three identities explicit:

| Identity | Purpose | Reuse rule |
|---|---|---|
| Revision-bound handle | Address a current declaration and admit an action | Refresh before acting on another revision |
| Object digest | Deduplicate and verify immutable content | Reuse identical content after digest verification |
| Correspondence between revisions | Relate a prior entity to current candidates | Report matched, ambiguous or missing; validate before rebinding |

Treat the code model as a graph with shared dependencies and cycles. Merkle objects can encode
acyclic snapshots and component summaries; graph references need an explicit canonical representation.
Avoid recursively hashing cyclic call edges as a tree.

Reuse a task step only when its recorded dependencies and analysis assumptions still hold.
Rebuild when that dependency set is incomplete. Compare incremental answers with clean rebuilds.
Measure cold, warm and single-edit costs before choosing finer cache granularity or a daemon.

### Changes, translation and proofs

Use separate representations with explicit bridges: source syntax, project relationships,
control/value analysis, portable authoring IR, application IR and formal kernels.
The existing authoring IR serves code generation. New analysis semantics should preserve
language-specific behavior and origin mappings where portable authoring cannot express them.

Feature work needs contracts, consumers, configuration and tests in the impact set. Bug work needs
a reproducer, competing explanations and regression evidence. Refactoring needs preservation claims
and affected call sites. Translation needs an explicit equivalence domain and unsupported constructs.
Numeric overflow, exceptions, evaluation order, mutation and effects belong in those contracts.

For existing code, prove a selected model property and state the source correspondence obligation.
For new code, author a specification and implementation together, with a checked bridge where feasible.
For refactoring or translation, retain the old and new models and state preservation or refinement.
Source changes must invalidate dependent proof evidence; renewing a hash cannot re-establish a theorem's applicability.

Lean work belongs in every relevant outcome. Use small executable kernels for edit admission,
invalidation and plan transitions. Keep parser, compiler, filesystem and subprocess assumptions
visible. A verified-generation path or source correspondence proof needs its own acceptance gate.

## Task acceptance matrix

These are proposed additions to the existing corpus. Pin independent oracles before implementation.

| Task | Required evidence | Acceptance example |
|---|---|---|
| Understand a subsystem | Relevant declarations, relationships, explanations and coverage | Answer an architectural question with exact evidence and explicit unknowns |
| Trace code flow | Call sites, branch conditions and dispatch candidates | Explain two paths with different behavior and preserve unresolved edges |
| Trace dataflow | Definitions, uses, call summaries and effects | Follow a value through a helper and distinguish a later overwrite |
| Sources and sinks | Rule identities, propagation, sanitizers and witness paths | Find an unsafe path and distinguish a sanitizer that applies in another context |
| Fix a bug | Failing reproducer, hypothesis evidence and affected consumers | Discover the target from a symptom, repair it and pass a separate regression oracle |
| Write a feature | Acceptance criteria, insertion points, contracts and callers | Add behavior across modules without supplying the edit target in advance |
| Refactor or restructure | Complete target set, ambiguity handling and preservation evidence | Move or change a declaration and update its consumers through one reviewed delivery |
| Translate code | Source/target semantics, fidelity gaps and correspondence evidence | Compare both implementations on declared cases and report unmodeled behavior |
| Prove old or new code | Property, assumptions, anchors and source/model relationship | Check a property, change a dependency and reject the stale evidence |
| Resume a task | Retained evidence, dependency changes and invalidated steps | Continue after a relevant edit while retaining independently valid work |

Use positive, negative, ambiguous and truncated cases. Measure useful task completion, false claims,
diagnostic precision, context bytes, source reveal, latency and recomputation. Record tokens and
cache usage only when the runner exposes them. Compare against ordinary search plus source editing
and against the current `fr` workflow on the same task and revision.

Allow bounded follow-up discovery during investigation. The old requirement for zero exploratory
calls after guidance favors preselected tasks. Keep it as a historical cohort condition, while new
investigation trials measure whether each discovery contributes useful evidence.

## Changes in understanding

- Representative delivery is a completed milestone within a continuing analysis product.
- Declaration spans solve target navigation; occurrence and semantic origin maps complete the text/AST connection.
- Call reachability, value propagation and security taint need separate claims and models.
- Merkle storage supplies integrity and reuse of content; dependency validation supplies reuse of conclusions.
- Structured routing and immutable reviews provide the base for a persistent investigation plan.
- Proofs of model properties, finite agreement and source implementation proofs remain separate evidence classes.
- Runtime type validation belongs at JSON and tool boundaries. Prioritize typed internal contracts over counting `isinstance` calls.

## Review scope and evidence

This review reads the implementation, protocol guides, roadmap, handoff and retained acceptance registry.
It runs the live audit, workflow audit, proof audit and capability report from the existing local binary.
A small Rust fixture confirms exact declaration locations and the explicit parameter-flow boundary.
These checks assess contracts; they do not replay compiler, behavioral or Lean acceptance suites.
The retained representative registry audit passes: six deterministic cases, ten live trials and
two matched cohorts. The ten capability-matrix tests, local documentation links, prose budget and
diff whitespace checks pass for this documentation update.

Key implementation entry points:

- [`src/project.rs`](../src/project.rs), [`src/span.rs`](../src/span.rs): handles, maps and declaration locations.
- [`src/project/evidence.rs`](../src/project/evidence.rs), [`src/project/relationships.rs`](../src/project/relationships.rs): bounded evidence and relationships.
- [`src/analysis/flow.rs`](../src/analysis/flow.rs), [`src/analysis/call_graph.rs`](../src/analysis/call_graph.rs): local flow and call reachability.
- [`src/cache.rs`](../src/cache.rs), [`src/index.rs`](../src/index.rs): fact and workspace resolution reuse.
- [`src/project/agent_guide.rs`](../src/project/agent_guide.rs), [`src/project/task.rs`](../src/project/task.rs): goal routing and task manifests.
- [Formalization contract](agent-formalization.md): admitted kernels and correspondence limits.
- [Representative registry](../tests/agent-eval/representative-acceptance.json): pinned task and live-trial evidence.

Documentation changes use direct editing. The admitted structural routes do not express the
complete multi-document roadmap rewrite. No product behavior or retained evaluation result changes.
