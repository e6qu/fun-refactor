# Agent analysis and planning roadmap

Build `fr` into an agent's static analysis, code map and change engine. An agent should start from
a question or task, discover relevant evidence progressively, plan its work and deliver checked results.

This file tracks unfinished outcomes and their completion gates. The [architecture review](docs/agent-analysis-review.md)
holds the baseline and design rationale. The [documentation map](docs/README.md) describes current features.
Use `fr --json audit` and `fr capabilities` for live support and refusal reasons.

## Completion criteria

The roadmap is complete when all four milestones pass their gates within explicit language and
semantic subsets. The acceptance corpus must cover:

- Code understanding and control flow, with exact evidence and unresolved relationships.
- Dataflow and sources/sinks, with bounded witnesses and explicit analysis assumptions.
- Feature writing and bug fixing from requirements or symptoms without a supplied edit target.
- Refactoring and structural changes with affected consumers and preservation checks.
- Translation with declared source/target semantics and independent behavioral checks.
- Lean properties for existing and new code, including explicit source correspondence obligations.
- Task resumption after interruption and code changes, with invalidation of dependent evidence.

Each case needs a pinned repository revision, declared scope, independent oracle and retained result.
An accepted task establishes its stated outcome. Unsupported behavior remains an explicit boundary.
Earlier guided delivery results remain regression evidence in the [evaluation registry](tests/agent-eval/representative-acceptance.json).

## Milestones and order

| Milestone | Status | Dependency | Completion evidence |
|---|---|---|---|
| A. Exact evidence and investigation | In progress | Existing discovery and delivery | Unknown-target bug and feature tasks |
| B. Control flow, dataflow and sources/sinks | In progress | A's occurrence and evidence contracts | Independently checked flow witnesses and negative cases |
| C. Incremental analysis and resumable plans | In progress | A's task contract; dependency records for each reused analysis | Resume tests and agreement with clean rebuilds |
| D. Changes, translation and proof obligations | In progress | Relevant evidence from A and B | Checked changes, translations and old/new proof tasks |

Start with A. Build the smallest useful task plan there and add persistence and invalidation in C.
C and D can advance when their specific prerequisites exist. Formal verification and evaluation
accompany every milestone.

### A. Exact evidence and investigation

Outcome: an agent can find relevant code and explain a proposed change from exact, bounded evidence.

- [x] Expose exact locations for relationship endpoints, references, call sites and flow occurrences.
  Distinguish occurrences from their declarations and preserve typed Rust/Python contracts.
  See the [contract](docs/agent-investigations.md) and [passing integration evidence](tests/investigation.rs).
- [x] Map semantic nodes to source origins. Represent absent and multiple origins explicitly.
  [Paged Python body provenance](src/project/semantic_origins.rs) links semantic and authoring pointers to syntax.
  [Independent coordinate tests](tests/semantic_evidence.rs) cover repeated calls, combined origins and missing mappings.
- [x] Attach analysis rules, input identities, scope, confidence and omissions to facts.
  Offer bounded explanations and follow actions through existing progressive discovery.
  [Python scalar fact pages](src/project/flow_facts.rs) retain separate analysis and disclosure coverage.
  [Independent acceptance](tests/agent-eval/results/2026-09-24-flow-facts/result.json) checks exact links, negative cases and stale refusals.
- [x] Include relevant build configuration and external evidence in the analysis scope.
  Preserve toolchain identity and disagreements when a task requires compiler facts.
  [Rust compiler evidence](docs/project-checks.md#retained-compiler-diagnostics) binds declared invocations and workspace build inputs.
  [Real rustc/Cargo acceptance](tests/agent-eval/results/2026-09-24-compiler-evidence/result.json) preserves exact diagnostics, disagreements and stale refusals.
- [x] Add a typed task plan containing acceptance criteria, hypotheses, evidence references,
  dependencies, unresolved questions and required checks. Reuse guide actions and reviewed delivery.
  [Typed plans](sdk/python/src/fr_ir/investigation.py) retain guide inputs and bind required checks to declared dependencies.
  [Check attachment](sdk/python/src/fr_ir/investigation_checks.py) preserves reviewed execution and rejects stale evidence.
- [ ] Pin one bug reproducer and one feature requirement with initially unknown edit targets.
  Measure the current workflow before extending it.

Gate: agents discover the relevant occurrences and support their decisions with retained evidence.
Independent oracles check the diagnosis and final behavior. Tests distinguish same-line calls,
shadowing, Unicode positions, missing origins, stale revisions and truncated discovery.

### B. Control flow, dataflow and sources/sinks

Outcome: an agent can explain how control and values reach a use across admitted function boundaries.

- [ ] Define analysis semantics for the language subset required by A's tasks.
  Connect the analysis representation to syntax and authoring IR through explicit origin mappings.
- [x] Model control-flow blocks, assignment order, definitions/uses, branches, loops and returns.
  Model admitted exceptional exits and report unsupported effects.
  The [Python scalar graph](src/project/control_flow.rs) admits while loops and explicit raises;
  [independent oracles and boundary tests](tests/flow_fixed_point.rs) cover the declared subset.
- [x] Add function summaries for argument/parameter transfer, return values and effects.
  Declare context, recursion, alias and resource policies; preserve unknown external-call boundaries.
  [Symbolic summaries](src/project/flow_summaries.rs) solve direct and mutual recursion within the Python scalar subset.
  [Independent evidence](tests/agent-eval/results/2026-09-24-recursive-flow/result.json) covers transfer, sink effects, explicit raises and budget boundaries.
- [x] Add versioned source, sink, propagation and sanitizer rules with context-specific contracts.
  Return witnesses with exact occurrences, rule identities, assumptions and cutoffs.
  See the [analysis implementation](src/project/dataflow.rs) and [rule tests](tests/investigation.rs).
- [x] State whether each result describes possible behavior, a proved condition or a heuristic candidate.
  Keep call reachability, value propagation and feasible paths distinct.
  The [contract](docs/agent-investigations.md) separates graph reachability, may-value derivations and model proofs.

Gate: trace through a helper, distinguish overwrites and retain branch alternatives.
Cover loops, recursion, aliases and unknown external calls within the declared subset.
Find an unsafe path and distinguish a sanitizer that applies in another context.
Independent positive and negative fixtures measure accuracy; exhausted budgets report incomplete analysis.

### C. Incremental analysis and resumable plans

Outcome: an agent can retain useful work across revisions without reusing stale conclusions or actions.

- [x] Separate revision-bound handles, immutable Merkle object digests and correspondence between revisions.
  Report matched, ambiguous and missing correspondence before rebinding targets.
  [Durable target sessions](sdk/python/src/fr_ir/investigation_session.py) require explicit fresh selections and dependencies.
  [Retained acceptance](tests/agent-eval/results/2026-09-24-resumable-correspondence/result.json) covers ambiguity, moves, renames and stale review refusal.
- [x] Record dependencies on source, imports, configuration, analyzer versions and external summaries.
  Track negative lookups whose results can change when new declarations appear.
  [Static Python module closures](src/project/flow_modules.rs) bind source, import candidates, missing members and rule contracts.
  [Retained acceptance](tests/agent-eval/results/2026-09-25-imported-flow/result.json) covers clean agreement and dependency-bound plan resumption.
  This contract admits root-local modules; packages and runtime import machinery remain outside its scope.
- [x] Define canonical graph records and cycle handling. Reuse existing object stores and caches.
  Fall back to a complete rebuild when dependency coverage is insufficient.
  [FlowCache](sdk/python/src/fr_ir/flow.py) uses verified Merkle records with local graph references.
  [Reuse tests](sdk/python/tests/test_flow.py) cover clean equivalence, invalidation, tampering and incomplete rebuilds.
- [x] Persist task plans locally with pending, ready, running, satisfied, blocked and stale steps.
  Invalidate dependent evidence and refresh prerequisites before resuming work.
  See [native resumption tests](tests/investigation.rs) and [verified SDK persistence](sdk/python/tests/test_investigation.py).
- [x] Bind checks and proof results to their inputs. Retain the existing immutable mutation review boundary.
  [Retained model proofs](src/spec/retained.rs) bind generated local Lean packages, source snapshots and checker identities.
  [Acceptance](tests/agent-eval/results/2026-09-24-retained-proofs/result.json) covers module checks, invalidation and fresh reviewed delivery.
  External Lean packages and source implementation correspondence remain outside this proof contract.
- [ ] Measure cold, warm and single-edit latency, memory, context bytes and recomputation.
  Choose cache granularity from those measurements.

Gate: reopen an interrupted task, preserve independent evidence and invalidate a changed dependency.
Exercise renames, moves, deletions, added overloads, configuration drift and analyzer changes.
Incremental answers agree with clean rebuilds. Tampered objects and stale reviews cannot admit writes.

### D. Changes, translation and proof obligations

Outcome: an agent can implement the requested task and state precisely what its checks and proofs establish.

- [ ] Discover affected consumers, contracts, configuration and tests for feature, bug and structural changes.
  Deliver the complete change through existing review, history and patch mechanisms.
- [ ] Expand semantic and application IR only for pinned task requirements.
  Define transport, effects, success and failure behavior for each new construct.
- [ ] Define translation domains for arithmetic, exceptions, evaluation order, mutation and effects.
  Preserve fidelity gaps and compare source/target behavior with independent oracles.
- [ ] For existing code, retain the property, model and source correspondence obligations.
  For new code, author the specification and implementation together.
- [ ] Retain old/new models for refactoring and translation, with preservation or refinement claims.
  Reject dependent proof evidence after relevant changes.
- [ ] Check small executable kernels for edit admission, dependency invalidation and task transitions.
  State trusted components and remaining obligations; use fault injection for host operations.

Gate: admitted constructs have compatibility cases, explicit refusals and independent behavior checks.
Pinned tasks cover structural changes, translation, existing-code proofs and new-code specification/proof work.
Strict verification rejects stale evidence and hidden proof debt. Delivery passes reversal and patch replay.
Source implementation claims require a checked correspondence argument or verified generation path.

## Current implementation and remaining gates

The [investigation contract](docs/agent-investigations.md) documents exact relationship/reference/flow
occurrences, bounded Python scalar propagation, dependency-bound local plans, immutable plan storage,
syntactic correspondence and the expanded Python reviewed writer. The
[integration tests](tests/investigation.rs) and [SDK tests](sdk/python/tests/test_investigation.py)
exercise those routes. The [deterministic evaluator](tools/investigation-acceptance.py) pins unknown-target
bug and feature requirements, reviewed repair/insertion, reversal and independent patch replay.
The [retained acceptance](tests/agent-eval/results/2026-09-23-investigation-acceptance/manifest.json)
binds the evaluator, pinned source revision, fixture and passing results.

These are substantial implementation advances across A–D; the complete milestone gates remain open:

- A: normalization origins, additional languages and compiler adapters, broader dependency discovery,
  fact explanations beyond scalar analysis and live unknown-target trials.
- B: source correspondence beyond exact syntax-origin links, package imports, implicit exception/handler semantics
  and a broader independently measured positive/negative corpus. While loops now reach a bounded
  fixed point; incomplete work never establishes absence.
- C: dependency coverage across package and dynamic imports, finer summary reuse, comprehensive incremental/clean
  rebuild comparison and representative measurements. Durable syntax targets now support explicit refresh after moves or renames.
  Opt-in whole-file flow reuse now validates
  source, rules, configuration and analyzer inputs, then renews occurrences after unrelated edits.
  Static root-local imports now extend that validated source closure and track missing import candidates.
- D: new translation domains and old/new source correspondence proofs, plus host fault injection.
  Existing translation/proof acceptance remains regression evidence, not proof of these new outcomes.

No milestone is complete. The remaining checklists retain their full outcome requirements; a partial
implementation does not close a multi-part item. Extend the pinned corpus and close these gates in
substantial integrated deliveries with the same evidence discipline.

The [fixed-point corpus](tests/agent-eval/flow-fixed-point/task.json) pins a delayed loop flow and
negative control-transfer cases. The [evaluator](tools/flow-acceptance.py) compares cold, warm and
single-edit results with clean analysis, including isolated peak RSS and transfer counts.
The [Flow kernel](kernels/FrKernels/Flow.lean) proves join/overwrite laws; native joins match 1,024 model cases.

The [semantic evidence task](tests/agent-eval/semantic-evidence/task.json) pins Python AST coordinates and declared checks.
Its [evaluator](tools/semantic-evidence-acceptance.py) compares source reveal with origin lookup and retains passing and failing command evidence.
Checked plans cover workspace, source, configuration and executable identities. Declared environment keys and external files now enter toolchain freshness checks.

The [recursive flow task](tests/agent-eval/recursive-flow/task.json) pins positive and negative recursive helpers.
Its [evaluator](tools/recursive-flow-acceptance.py) retains runtime and coordinate oracles plus cold, warm and helper-edit comparisons.
The opt-in solver covers symbolic positional parameters and explicit scalar effects. Whole-file reuse remains conservative;
short-circuit call control and heap effects remain outside the admitted subset.
Static local imports now have a separate opt-in contract; package loading remains outside it.

The [flow fact task](tests/agent-eval/flow-facts/task.json) pins bounded explanations and exact semantic/authoring links.
Its [evaluator](tools/flow-facts-acceptance.py) retains paged evidence, independent Unicode coordinates,
negative cases and stale refusals. Semantic mappings remain explicit when absent or truncated.
The language semantics item in B remains open pending the unknown-target task requirements and broader correspondence gates.

The [compiler evidence task](tests/agent-eval/compiler-evidence/task.json) retains Rust diagnostics and explicit syntax/compiler differences.
Workspace build-input discovery tracks added configuration files. Environment and external toolchain coverage remains declaration-based;
undeclared dependencies and source correspondence proofs remain open.

The [correspondence task](tests/agent-eval/resumable-correspondence/task.json) pins durable declaration identities and interrupted resumption.
Its [evaluator](tools/correspondence-acceptance.py) retains independent coordinates, ambiguity, invalidation and fresh checked patch delivery.
Target refresh clears old actions and evidence. Content equality never authorizes a stale mutation review.
Measured fixture latency and memory do not settle the representative cache-granularity gate in C.

The [imported-flow task](tests/agent-eval/imported-flow/task.json) pins transitive helpers, independent runtime outcomes and exact cross-file coordinates.
Its [evaluator](tools/imported-flow-acceptance.py) records missing lookups, dependency drift, plan resumption and fresh reviewed patch delivery.
Cold, warm and edited measurements retain clean-analysis agreement. They do not settle the representative cache-granularity gate.

## Rules for every milestone

- Keep source syntax, project relationships, analysis, authoring IR and formal models connected through explicit identities.
- Bind text locations and evidence to the selected snapshot. Require explicit bounded actions for source reveal.
- Preserve coverage, assumptions and omissions. Truncation and unknown effects cannot establish absence or safety.
- Validate dependencies before reusing conclusions. Content equality alone cannot authorize a stale action.
- Keep agent hypotheses distinct from tool facts. Task completion requires evidence for acceptance criteria.
- Review every complete mutation before execution. Preserve unrelated user changes, recovery and reversal.
- Preserve workspace boundaries, ambiguity refusals and independently verifiable canonical Merkle records.
- Distinguish syntax, compilation, tested behavior, model theorems and source implementation correspondence.

## Delivery and acceptance process

Before each slice, pin its task, language subset, budgets and oracle. Include positive, negative,
ambiguous, incomplete and stale cases. Implement code, typed contracts, tests, models and docs together.
Use `fr` for repository changes when an admitted route exists; retain refusals or direct-edit boundaries.

Compare against ordinary search/source editing and the current `fr` workflow under equivalent conditions.
Record task success, false claims, useful discoveries, source reveals, context bytes and latency.
Record tokens only when available. Allow bounded follow-up discovery during investigation.

Keep deterministic checks, analysis accuracy and live-agent results separate. Retain failed attempts.
Expand live trials across repositories and task shapes; report their scope without extrapolating population claims.
Audit retained evidence before running fresh paid trials.

Run focused checks during implementation, then the affected complete gates:

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Follow the [development guide](docs/development.md) for toolchains and resource limits,
the [SDK contract](sdk/python/README.md) for Python conventions, and the
[evaluation guide](docs/evaluations.md) for retained evidence. Generated code also needs its declared compiler or runtime oracle.

Mark a checklist item complete only with linked implementation and passing evidence.
Mark a milestone complete only after its gate passes; record unresolved work explicitly.
Keep this file focused on remaining work and move completed detail into the review or evaluation records.
Defer new parsers, distribution channels and daemon infrastructure until a pinned task or measurement requires them.
