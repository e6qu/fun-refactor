# Earlier review of code understanding and task planning

Use this dated review to understand why the project moved from applying known edits toward helping agents investigate unfamiliar problems.
The [current plan](../PLAN.md) owns priorities. The [status report](roadmap-status.md) owns current results and unfinished requirements.

## Dated assessment and proposed work

The original review took place on 2026-09-23 at `c22ba827`, including PR #355.
The observations and proposed work below describe that point in development. They are not today's support list or new measurements.

## Assessment

The tool could already find code, prepare edits, require review and deliver checked patches in selected examples.
The missing larger outcome was helping an agent discover the right repair in a project it had never seen.

For example, “fix checkout rounding” needs more than a command that replaces a supplied function body.
The agent must find the calculation, explain the mistake, inspect its callers and choose tests that catch the requested error.
The review recommended connecting analysis to those decisions while preserving review, undo and explicit unanswered questions.

## What exists

A declaration defines a named piece of code; a reference uses that name.
A handle selects a declaration in a particular source state. A fingerprint identifies saved contents.
A model describes the parts of code the tool supports. These concepts serve different purposes.

| Area at the review date | What it helped the user do | What it did not establish |
| --- | --- | --- |
| Parsing and indexing | Recognize syntax, functions, uses and imports | Complete understanding of a language's behavior |
| Project maps and source locations | Find a declaration and open relevant source | That the code is correct or every related use is known |
| Calls and affected-code reports | List possible callers and related changes | Which runtime target executes or which argument reaches the result |
| Selected report pages and content storage | Read needed sections and verify saved bytes | That a saved conclusion still applies after another file changes |
| Guides and reviewed changes | Choose a supported command and inspect an edit before applying it | That the agent discovered the right solution |
| History, checks and patch delivery | Test, reverse and transfer a known change | Behavior outside the chosen tests |
| Lean proof support | Check a stated rule about a supported model | An automatic proof of the original source implementation |

Use the installed `fr audit` for current support. Historical counts in this review cannot replace it.

## Gaps that affect agent work

### Exact occurrences and analysis provenance

An occurrence is one written use, with a file and range of source positions.
If `send(first()); send(second())` appears on one line, the explanation must identify the particular call it means.
A declaration's location alone cannot distinguish those uses.

The review proposed recording each use's source state, path, byte range, line range and role.
Generated model nodes can have missing or multiple source locations; the tool must report those cases rather than invent a location.

Provenance means where a finding comes from. A useful explanation includes the rule, inputs, assumptions and unexamined parts behind it.
For a compiler finding, retain its build settings and tool version too.
A source search suggesting a related test differs from evidence that the test exercised the requested behavior.

### Control flow and value flow

Control flow describes which steps can follow which others. Value flow describes where an input can contribute to a later value.
A call list only says which functions may be invoked; it cannot establish which argument a helper returns.

For example, `choose(left, right)` returning `left` should treat the two input slots differently.
Overwriting a value before `send` should also differ from forwarding it unchanged.
The proposed work was to represent evaluation order, assignments, branches, loops, returns and supported exceptions explicitly.

A function summary records those supported relationships across calls, including recursive calls.
Unknown calls and shared mutable objects need explicit limits.
Repeating analysis until no new facts appear does not prove that the program terminates.

For input/output questions, users name external source, sink and cleaning rules.
A source supplies the tracked value; a sink consumes it. Cleaning is specific to a context, such as HTML output.
A matching function name alone cannot establish safety.
Return possible connections with their exact locations and assumptions. An unfinished search must leave absence unanswered.

### Task planning

A task plan keeps the requested outcome, questions, possible explanations, observations and remaining checks.
For checkout, keep “the caller passes cents” as an observation and “the conversion is wrong” as a hypothesis until checked.

Each proposed step needed:

- A question or intended outcome and a local ID.
- The source and settings its findings depend on.
- The read, proposed edit, test or proof needed next.
- Its state, such as pending, running, satisfied or outdated.
- The evidence satisfying the requirement and anything still unknown.

Saving a plan should not give it permission to apply an unreviewed edit.
The recommendation was to test saving, interruption, reopening and source changes before adding scheduling or concurrent work.

### Merkle reuse across revisions

A Merkle tree organizes content fingerprints so a changed child changes the fingerprints above it.
That can verify stored report bytes. It cannot by itself establish that an earlier answer applies to today's code.

For example, an unchanged helper can behave differently if its import now selects another function.
Reuse must check imports, relevant build settings, called functions and previously missing names as well as the helper's own contents.

| Reference | What it identifies | What to do after an edit |
| --- | --- | --- |
| Current handle | Selected code in a particular source state | Obtain a fresh one before acting |
| Object digest | Saved contents with a particular fingerprint | Verify the contents before reading them |
| Candidate match across revisions | Code that might correspond to an earlier declaration | Review ambiguous or missing matches; never renew edit permission automatically |

Compare reused answers with fresh analysis. Measure initial work, reuse, rechecking after an edit, storage and memory.
The [measurement guide](benchmarks.md) explains these baselines and costs.

### Changes, translation and proofs

Different questions need different descriptions of code.
The written syntax supports source edits; call relationships help find affected code; value rules help answer input questions.
IR, the shared structured code record, supports selected generation and translation.
A proof model supplies the definitions and assumptions checked by Lean.

For a bug, require a reproducing example and an independent behavior test.
For a signature change, inspect affected callers. For translation, state which inputs, arithmetic, errors and effects must agree.

A proof establishes its stated property under its assumptions.
Check its connection to the source implementation separately, and discard dependent proof results after relevant source changes.
Changing a fingerprint cannot re-establish that connection.
Small proofs about the tool's review or recovery rules remain useful, with parser and filesystem assumptions visible.

## Task acceptance matrix

These were proposed tests, not completed experiments. Choose the independent reference checks before implementing the feature.

| User task | Example result the test should require |
| --- | --- |
| Understand a subsystem | Explain an architecture question using the relevant code and explicit unknowns |
| Follow execution | Explain two branches with different behavior and keep unresolved call targets visible |
| Follow a value | Trace input through a helper and distinguish an overwrite |
| Investigate unsafe output | Distinguish HTML cleaning from cleaning intended for another context |
| Fix a bug | Find the responsible code from a symptom and pass an independent regression test |
| Add a feature | Meet its requirements across modules without being handed the edit location |
| Refactor | Update the promised declaration and callers through one reviewed change |
| Translate | Compare source and generated behavior for stated inputs and report unsupported behavior |
| Prove a property | State assumptions, check the proof and establish its connection to the selected code |
| Resume work | Recheck affected findings after a helper changes while preserving unrelated observations |

Include reaching-input, blocked-input, ambiguous and truncated cases.
Measure correctness and all work spent, including failures and follow-up reads.
Compare with ordinary search and editing on the same task and revision, and report missing time or cost information as unknown.

## Changes in understanding

The review separated several claims that earlier summaries could blur.
Applying a prescribed patch differs from discovering a solution.
Finding a possible caller differs from tracing a particular value.
Verifying saved bytes differs from rechecking a conclusion after an import changes.
A proof of a separate model differs from a proof connected to the implementation.
These distinctions still guide how current results should be explained.

## Review scope and evidence

At the original review, the author inspected source, protocol guides, planning pages and the saved acceptance registry.
The local audit commands and a small Rust example checked support reports and declaration locations.
They did not replay the compiler, behavior or Lean acceptance suites.

The original registry audit reported six scripted cases, ten live trials and two matched groups of attempts.
Ten capability-report tests, local links, writing counts and whitespace checks passed for that earlier documentation update.
Those historical observations remain separate from validation of this rewrite and from today's larger product goals.

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
