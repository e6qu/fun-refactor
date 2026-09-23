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

- [ ] Expose exact locations for relationship endpoints, references, call sites and flow occurrences.
  Distinguish each occurrence from its containing declaration; preserve typed Rust/Python contracts.
- [ ] Map semantic nodes to source origins. Represent absent and multiple origins explicitly.
- [ ] Attach analysis rules, input identities, scope, confidence and omissions to facts.
  Offer bounded explanations and follow actions through existing progressive discovery.
- [ ] Include relevant build configuration and external evidence in the analysis scope.
  Preserve toolchain identity and disagreements when a task requires compiler facts.
- [ ] Add a typed task plan containing acceptance criteria, hypotheses, evidence references,
  dependencies, unresolved questions and required checks. Reuse guide actions and reviewed delivery.
- [ ] Pin one bug reproducer and one feature requirement with initially unknown edit targets.
  Measure the current workflow before extending it.

Gate: agents discover the relevant occurrences and support their decisions with retained evidence.
Independent oracles check the diagnosis and final behavior. Tests distinguish same-line calls,
shadowing, Unicode positions, missing origins, stale revisions and truncated discovery.

### B. Control flow, dataflow and sources/sinks

Outcome: an agent can explain how control and values reach a use across admitted function boundaries.

- [ ] Define analysis semantics for the language subset required by A's tasks.
  Connect the analysis representation to syntax and authoring IR through explicit origin mappings.
- [ ] Model control-flow blocks, assignment order, definitions/uses, branches, loops and returns.
  Model admitted exceptional exits and report unsupported effects.
- [ ] Add function summaries for argument/parameter transfer, return values and effects.
  Declare context, recursion, alias and resource policies; preserve unknown external-call boundaries.
- [ ] Add versioned source, sink, propagation and sanitizer rules with context-specific contracts.
  Return witnesses with exact occurrences, rule identities, assumptions and cutoffs.
- [ ] State whether each result describes possible behavior, a proved condition or a heuristic candidate.
  Keep call reachability, value propagation and feasible paths distinct.

Gate: trace through a helper, distinguish overwrites and retain branch alternatives.
Cover loops, recursion, aliases and unknown external calls within the declared subset.
Find an unsafe path and distinguish a sanitizer that applies in another context.
Independent positive and negative fixtures measure accuracy; exhausted budgets report incomplete analysis.

### C. Incremental analysis and resumable plans

Outcome: an agent can retain useful work across revisions without reusing stale conclusions or actions.

- [ ] Separate revision-bound handles, immutable Merkle object digests and correspondence between revisions.
  Report matched, ambiguous and missing correspondence before rebinding targets.
- [ ] Record dependencies on source, imports, configuration, analyzer versions and external summaries.
  Track negative lookups whose results can change when new declarations appear.
- [ ] Define canonical graph records and cycle handling. Reuse existing object stores and caches.
  Fall back to a complete rebuild when dependency coverage is insufficient.
- [ ] Persist task plans locally with pending, ready, running, satisfied, blocked and stale steps.
  Invalidate dependent evidence and refresh prerequisites before resuming work.
- [ ] Bind checks and proof results to their inputs. Retain the existing immutable mutation review boundary.
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

These are substantial implementation advances across A–D; the complete milestone gates remain open:

- A: semantic-node origin maps, compiler/build evidence and live unknown-target investigation trials.
- B: an explicit control-flow graph, convergent loop/recursive summaries, exceptional exits and a
  broader independently measured positive/negative corpus. Current loop witnesses report incomplete.
- C: automatic semantic dependency coverage, analysis-result reuse, comprehensive incremental/clean
  rebuild comparison and representative latency/memory measurements. Plans currently reuse validated
  evidence; they do not implement an incremental dataflow cache.
- D: new translation domains and old/new source correspondence proofs, plus host fault injection.
  Existing translation/proof acceptance remains regression evidence, not proof of these new outcomes.

No milestone is complete. The remaining checklists retain their full outcome requirements; a partial
implementation does not close a multi-part item. Extend the pinned corpus and close these gates in
substantial integrated deliveries with the same evidence discipline.

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
