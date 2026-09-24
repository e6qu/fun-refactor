# Evidence and resumable investigations

`project calls`, `project show --relations` and `project dataflow` expose source occurrences.
An occurrence carries a revision, relative path, half-open UTF-8 span, matching 1-based Unicode
range, role and enclosing declaration handle. Its `fro1` identity distinguishes two calls on one
line. Relationship endpoints retain declaration locations separately. A call offset without an
indexed reference has an absent origin; conflicting spans have multiple origins. Neither case
creates an invented range. `Occurrence.from_data` and `Occurrence.text` provide typed Python access.

Evidence disclosure uses a compact flow record: path, exact byte span, start position and occurrence
ID, bound to its disclosure revision. Declaration details remain accessible through their handles.

## Semantic origins

`project semantic HANDLE --body --origins` pages origins beside the existing semantic model.
Use `--origin-limit` (1–256), `--origin-cursor` or `--origin-pointer` for bounded follow-up queries.
The `origins.continuation` action retains the snapshot and page identity. A changed revision refuses its cursor.
Each record binds a semantic node ID, model pointer, optional authoring-body pointer, rule and source origins.
The report retains analyzer and input digests, scope, confidence, omissions and follow actions.

The admitted mapping covers Python function bodies that remain unchanged through normalization.
Supported statements and scalar expressions keep their exact syntax spans. Identical calls on one line remain separate.
An augmented assignment's synthesized binary expression names both contributing operand spans.
The null expression generated for `pass` has an absent origin. Unsupported mappings also remain absent.
File-wide queries and other languages retain absent origins. Body pointers require an available authoring-body identity.
Normalization can change the model's structure; such bodies currently keep absent origins throughout.
Provenance explains where a node came from. It does not establish behavioral equivalence or compiler semantics.
Page completeness describes disclosure, not completeness of the origin mapping or runtime analysis.

`SemanticOrigins.inspect` in `fr_ir.origins` validates the typed report. `for_body_pointer` links authoring positions;
`for_occurrence` matches flow or call evidence by revision, path and exact span. Multiple matches remain visible.
Source text still requires explicit bounded disclosure or a caller-supplied snapshot through `Occurrence.text`.

## Scalar flow

Select a fresh top-level Python function, then trace it with an explicit budget:

```sh
fr --json project find checkout
fr --json project dataflow '<HANDLE>' --steps 256 --depth 8
fr --json project dataflow '<HANDLE>' --rules rules.json --context html
```

The `python-scalar-fixed-point-2` subset covers scalar parameters, literals, identifier assignments,
arithmetic and Boolean expressions, branches, while loops, returns and direct same-file helper calls.
Assignment replaces prior origins. Joins union possible origins and intersect definitely bound names.
Helper arguments transfer to parameters and returned values. Reports retain exact control/use events,
return origins and source-to-sink witnesses. This is value propagation with unchecked feasibility;
it does not establish runtime call dispatch, absence of exceptions or security of an application.

Rules name external contracts and carry a required version:

```json
{
  "version": "application-contracts-1",
  "sources": ["read_input"],
  "sinks": ["render"],
  "propagators": ["identity"],
  "sanitizers": {"escape_html": "html"}
}
```

Rules cannot overlap each other or local definitions. A sanitizer removes origins from its return
only in its declared context. Rules are supplied assumptions, not inferred properties of functions.
Reports bind their complete rule content by digest. Entry parameters also begin as value origins.

The worklist follows explicit control-flow graphs until origin sets and definite bindings stabilize.
While loops include zero or more iterations. Break skips the loop's else suite; continue returns to
its condition. Elif clauses and explicit raises have separate edges. Return and raise terminate their
paths. Implicit exceptions, exception handlers, for loops and propagation of helper exceptions remain
unsupported. Annotations and unsupported lexical binding forms also retain explicit cutoffs.
Recursive calls, unknown
external calls, aliases, dynamic calls, unsupported syntax, module effects and exhausted budgets
report cutoffs. A partial report cannot establish absence. The route does not support implicit
control dependence, mutable objects or resource analysis. A complete report refers only
to the declared explicit-value subset and assumptions. The existing local `flow` route keeps its
separate contract.

Each control node carries an exact origin or an explicit absent origin for synthetic exits.
Edges refer to local node IDs, so cycles never require recursive JSON or Merkle records.
Graphs have at most 512 nodes per function. The step budget covers transfer and expression work;
the response budget reports omitted sections. Repeated sink/origin pairs retain one derivation.
These derivations are not executable paths: joins lose branch correlation, and conditions are not solved.
Context-specific evaluation summaries expose input origins, return origins, block visits and convergence.
The default route cuts off recursive calls. It does not claim runtime termination.

### Recursive function summaries

Add `--summaries` to use `python-scalar-summaries-1`. The solver summarizes each reachable function
with symbolic positional parameters. Each call substitutes its own arguments into return, sink and
exceptional effects. This keeps separate callers from sharing input values. Direct and mutual recursion
start with empty summaries and grow monotonically until a global fixed point or a cutoff.

`function_summaries` retains parameter transfer, exact derivations, callees, evaluation counts and convergence.
`completion.normal_return` distinguishes a constant return from a call with no modeled normal return.
Explicit helper raises propagate exceptional origins and can prevent subsequent statements from executing.
An absent normal return is a fact of the admitted model, not a proof of runtime divergence.
Branch feasibility and implicit Python exceptions remain outside the model.

The solver admits at most 64 functions and 512 control nodes per function. `--steps` covers all rounds.
`--depth` bounds discovery of new helpers; calls to already discovered recursive functions use their current summary.
An exhausted budget always makes the result incomplete. Unknown externals, aliases, annotations and
dynamic calls retain their cutoffs. Calls inside short-circuit expressions and exception causes also remain incomplete.
All existing rule context and module-effect boundaries still apply. Sources and sinks are external contracts;
the solver does not inspect imported implementations or track heap effects.

The [retained corpus](../tests/agent-eval/results/2026-09-24-recursive-flow/result.json) compares recursive
flow against independent Python execution and AST positions. Five Lean theorems establish parameter
selection and monotonicity in the finite-origin model. Native substitution matches all 1,024 executable
model cases. These theorems do not prove the parser, solver or Python implementation correspondence.

## Verified result reuse

`--inputs-only` reports the input identity without running flow transfer. It binds the defining file,
selection, rules, context, summary mode, budgets, analyzer implementation and indexed manifests/lockfiles.
The whole defining file covers helper bodies, local shadowing and negative same-file lookups.
Imported execution is outside the subset. Incomplete analyses and skipped configuration snapshots
cannot be reused. A changed input falls back to clean analysis.

The SDK uses the existing verified Merkle store:

```python
from fr_ir.context import DirectoryObjectStore
from fr_ir.flow import FlowCache

store = DirectoryObjectStore("/tmp/flow-objects")
cache = FlowCache(store)
analysis = cache.analyze(client, fresh_handle, steps=1024)
root = cache.persist()
restored = FlowCache.restore(store, root)
```

Keep the store outside the analyzed workspace. A cache holds at most 256 result roots.
`analysis.graphs` and `analysis.witnesses` provide typed access; `analysis.reused` reports reuse.
After unrelated edits, native validation renews occurrence identities, enclosing handles and revision
metadata. Retained results must equal clean analysis after that renewal. It never renews mutation reviews.

The native `--reuse FILE --reuse-digest SHA256` route accepts a retained JSON report and its trusted
canonical JSON digest. The SDK obtains both from a verified Merkle root. A digest establishes content
identity, not analytical truth or execution attestation. Do not accept an untrusted party's replacement
root as prior analysis. Tampering refuses; mismatched inputs and incomplete records recompute.

Reuse remains opt-in and coarse. It avoids flow transfer while retaining parsing, indexing, dependency
checks and occurrence renewal. Small inputs may cost more to restore than to analyze.
The [measurement tool](../tools/flow-acceptance.py) records cold, warm, unrelated-edit and relevant-edit
latency, child/worker peak RSS, report bytes and transfer steps in isolated workers. Tokens are unavailable.

The [Flow kernel](../kernels/FrKernels/Flow.lean) proves lattice join laws and overwrite properties.
The native join is checked against its executable model over 1,024 finite cases. This establishes
the tested correspondence, not a proof of the whole Python analyzer or host cache.

## Task plans

`fr project investigate --from plan.json` validates a local plan and returns its refreshed states.
It never executes an action or writes a plan file. The typed SDK persists plans through the existing
verified Merkle object store:

```python
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, TaskPlan, TaskStep
from fr_ir.runtime import FrClient

client = FrClient(".")
plan = TaskPlan("Explain a failing checkout", ("regression passes",), (
    TaskStep("diagnose", "Which input produces the wrong total?",
             (Dependency(DependencyKind.WORKSPACE, "selected-project"),),
             required_checks=("regression",), satisfies=("regression passes",)),
))
resumed = plan.resume(client, transition="diagnose:start")
store = DirectoryObjectStore("/tmp/investigation-objects")
digest = resumed.plan.store(store)
reopened = TaskPlan.restore(store, digest).resume(client)
```

A step carries dependencies, a question, guide/discovery arguments, required checks, retained
references and the acceptance criteria it satisfies. Hypotheses and unresolved questions remain
agent input. States are pending, ready, running, satisfied, blocked and stale. Interrupted running
steps reopen as ready; resumption cannot attest completion of an interrupted command. Explicit
transitions use `STEP:start`, `STEP:satisfy`, `STEP:block` and `STEP:reset`.

Dependencies cover indexed source, admitted configuration/lockfiles, name lookups (including empty
results), analyzer identity and whole-workspace identity. A missing digest captures an input only
for a new pending step. Changed dependencies invalidate evidence and propagate staleness through
the acyclic step graph. Reset explicitly clears a step's evidence and captures current inputs.
Use a workspace dependency whenever the complete semantic dependency set is unknown. Individual
source dependencies alone do not capture imports or configuration changes. Existing files outside
the admitted snapshot refuse dependency capture.

Evidence records bind a result kind, input digest, pass status and retained reference. Required
checks must have passing check records for that exact input digest. Model proofs, source
correspondence and observations have separate kinds. These are validated agent-reported records;
a reference is not an attestation that a tool ran. Completion requires covered acceptance criteria
and no unresolved questions. Neither completion nor content equality grants mutation authority.
All writes continue through existing immutable reviews, declared checks, history and patch delivery.

`TaskStep.from_guide` retains a ready guide action, its structured stdin and its workspace revision.
Plan resumption validates that revision before the action remains ready. Plans do not execute retained actions.
Use the existing guide review and task-change delivery APIs for mutations.

## Executed check evidence

`checks --toolchain` adds resolved executable paths, SHA-256 identities and the check-runner identity.
With `--run`, it compares those identities before and after execution alongside source and configuration stability.
It covers every configured command executable. Declare separate identity checks for interpreter imports,
dynamic libraries, environment variables or compiler version output when they matter to the task.
Compiler diagnostics and disagreements remain bounded command output with declared coverage.
The runner does not infer compiler facts from those diagnostics.

```python
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_checks import run_checks

plan = TaskPlan("Validate the repair", ("regression passes",), (
    TaskStep.checked("verify", "Does the regression pass?", checks=("regression",),
                     satisfies=("regression passes",)),
))
reviewed = client.call("checks", "--toolchain")
# Inspect the declared commands before execution.
result = run_checks(plan, client, "verify", reviewed, store)
```

Keep the Merkle store outside the analyzed workspace. `run_checks` requires an unchanged reviewed check listing.
It captures workspace, check configuration, complete check-source snapshot and executable identities as step dependencies.
The source snapshot includes ignored source files that the project index may omit.
The SDK retains passing and failing reports before attaching evidence. A failed check cannot satisfy acceptance.
A check that changes its inputs leaves a retained report and an attachment error; its dependent step becomes stale.
Missing configuration or executables refuse validation. Retained plan actions never gain write authority.

The native adapter accepts `project investigate --checks-from REPORT --checks-digest SHA256 --check-step ID`
alongside `--from PLAN`. It validates the trusted canonical report digest, declared commands, input identities and outcomes.
`restore_checks` reads a retained report through the verified object store. Reattachment revalidates current native inputs.
These digests bind trusted local records; they do not attest execution by an untrusted producer.
Model proofs and source-correspondence claims retain their separate evidence kinds and obligations.
The Investigation kernel checks the four-class dependency admission law against all sixteen native combinations.

## Correspondence and checked delivery

`project identities` pages immutable declaration digests beside current handles. Pass a retained
identity report with `--from` to obtain matched, ambiguous or missing candidates after a move or
edit. Identical content takes precedence; same-path/name/kind is a weaker fallback. Multiple equal
objects remain ambiguous. An unmatched rename can remain missing. These are syntactic candidates,
not a semantic equivalence proof or automatic rebinding. Only the supplied page participates.
Declaration objects exclude graph edges, so recursive references do not require recursive hashing.

Python task delivery now admits existing body replacement and insertion of one top-level function
into a file. Insertion reparses the complete result, refuses existing indexed binding names and
wildcard imports, and preserves all original bytes. Compiler/runtime checks remain necessary.

The [pinned corpus](../tests/agent-eval/investigation/task.json) supplies a negative-total symptom
and quote requirement without an edit target. The deterministic evaluator discovers a subtraction
through semantic evidence, performs reviewed changes, exercises reversal and replays both patches
in an independent receiver before running the behavioral oracle. It is not a live-agent trial.

The [integration suite](../tests/investigation.rs) covers occurrence identity, Unicode, bounded
pages, helper transfer, overwrites, branch alternatives, sanitizer contexts, explicit cutoffs,
dependency invalidation, correspondence and checked delivery. The SDK suite covers persistence and
coordinate validation. A Lean transition model proves that completion requires evidence and
prerequisites, with exhaustive agreement against the Rust admission function. This does not prove
filesystem integrity, parser correctness, the full plan host or source implementation semantics.

## Bounded flow facts

`project flow-facts HANDLE` projects the recursive scalar analysis into fact headers. Each header
binds a rule, input digest, function scope, confidence, assumptions, omissions and evidence digest.
`project explore` offers this route for top-level Python functions. External contracts still require
an explicit `--rules` file.

```sh
fr --json project flow-facts '<HANDLE>' --rules rules.json --context html --limit 2
fr --json project flow-facts '<HANDLE>' --rules rules.json --context html --fact '<FACT-ID>' --evidence-limit 2
```

Follow the returned actions to preserve context, budgets and identities. Fact and evidence pages
have separate cursors. Changed source revisions, contracts or analysis inputs refuse stale actions.
Restoring a stored page preserves its old identity; it cannot renew actions or authorize writes.

The report separates three questions:

- `analysis.complete` says whether the declared model finished without cutoffs or omitted analysis.
- `disclosure.complete` says whether this response contains the selected catalogue or explanation from its start to its end.
- Each evidence point's `mapping.complete` says whether its semantic origin lookup exhausted the admitted origin records.

Top-level `complete` combines analysis and disclosure coverage. An explanation can be complete while
some origin mappings are absent. A complete explanation covers one fact, not the entire catalogue.
A missing witness establishes model absence only after complete analysis and complete catalogue disclosure.
It does not establish security or path feasibility.

Explanations retain derivation occurrences and occurrence rules. They do not invent dependency edges
or executable paths. Exact revision, path and byte-span matches link occurrences to semantic origins
and authoring body pointers. These are syntax relations, not source implementation correspondence proofs.
Normalized or synthesized nodes can lack mappings. Multiple relations remain visible.

Each explanation page runs at most eight semantic queries. Each query admits 4,096 nodes and reads
at most 256 origin rows. Each point discloses at most four matching links. Partial queries retain
continuations and cannot report absent mappings. The semantic reader's source limit is 262,144 bytes.
Unavailable readers and exhausted budgets retain explicit gaps and bounded follow actions.
Source text appears only after an explicit source action. Fact pages default to 65,536 bytes;
`--bytes` accepts 4,096 through 1,048,576. The internal analysis also has a 1 MiB response limit.
Its omissions remain visible and prevent complete results.

The [retained acceptance](../tests/agent-eval/results/2026-09-24-flow-facts/result.json) compares raw
analysis and paged facts under the same contracts. The independent oracle executes nine cases and
checks UTF-8 AST coordinates. Full explanations add provenance and may exceed raw analysis in bytes
and latency. Four Lean theorems establish disclosure coverage policy; native code matches all eight
Boolean cases. These checks do not prove the analyzer or its source correspondence.
