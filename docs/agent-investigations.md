# Evidence and resumable investigations

`project calls`, `project show --relations` and evidence disclosure now expose source occurrences.
An occurrence carries a revision, relative path, half-open UTF-8 span, matching 1-based Unicode
range, role and enclosing declaration handle. Its `fro1` identity distinguishes two calls on one
line. Relationship endpoints retain declaration locations separately. A call offset without an
indexed reference has an absent origin; conflicting spans have multiple origins. Neither case
creates an invented range. `Occurrence.from_data` and `Occurrence.text` provide typed Python access.

## Scalar flow

Select a fresh top-level Python function, then trace it with an explicit budget:

```sh
fr --json project find checkout
fr --json project dataflow '<HANDLE>' --steps 256 --depth 8
fr --json project dataflow '<HANDLE>' --rules rules.json --context html
```

The `python-scalar-explicit-values-1` subset covers scalar parameters, literals, identifier
assignments, arithmetic and Boolean expressions, branches, returns and direct same-file helper
calls. Assignment replaces prior origins. Branch alternatives retain separate states, and helper
arguments transfer to parameters and returned values. Reports retain exact control/use events,
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

Loops retain zero/one-iteration witnesses and report an unchecked fixed point. Recursion, unknown
external calls, aliases, dynamic calls, unsupported syntax, module effects and exhausted budgets
report cutoffs. A partial report cannot establish absence. The route does not support implicit
control dependence, mutable objects, exceptions or resource analysis. A complete report refers only
to the declared explicit-value subset and assumptions. The existing local `flow` route keeps its
separate contract.

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
