# Evidence and resumable investigations

`project calls`, `project show --relations` and `project dataflow` expose source occurrences.
An occurrence carries a revision, relative path, half-open UTF-8 span, matching 1-based Unicode
range, role and enclosing declaration handle. Its `fro1` identity distinguishes two calls on one
line. Relationship endpoints retain declaration locations separately. A call offset without an
indexed reference has an absent origin; conflicting spans have multiple origins. Neither case
creates an invented range. `Occurrence.from_data` and `Occurrence.text` provide typed Python access.

Evidence disclosure uses a compact flow record: path, exact byte span, start position and occurrence
ID, bound to its disclosure revision. Declaration details remain accessible through their handles.

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
unsupported. Recursive calls, unknown
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
They do not implement recursive summaries or claim runtime termination.

## Verified result reuse

`--inputs-only` reports the input identity without running flow transfer. It binds the defining file,
selection, rules, context, budgets, analyzer implementation and indexed manifests/lockfiles.
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
