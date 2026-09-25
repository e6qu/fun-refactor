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
the default solver stays within one file and does not track heap effects.

The [retained corpus](../tests/agent-eval/results/2026-09-24-recursive-flow/result.json) compares recursive
flow against independent Python execution and AST positions. Five Lean theorems establish parameter
selection and monotonicity in the finite-origin model. Native substitution matches all 1,024 executable
model cases. These theorems do not prove the parser, solver or Python implementation correspondence.

### Static local imports

Add `--imports --summaries` to follow root-local Python modules. Both the selected file and admitted
modules live directly under the workspace root. Supported forms include `import helper`,
`import helper as alias`, `from helper import forward` and `from helper import forward as relay`.
Import module, member and alias names use ASCII identifiers.
Function identities include their file, such as `helper.py::forward`. Argument substitution and
explicit return, sink and raise effects retain exact occurrences across module boundaries.
`project flow-facts HANDLE --imports` exposes the same analysis through bounded explanations and semantic links.

The loader records each import declaration and its `.py`, `.pyi` and `__init__.py` candidates.
A source module requires absent package and stub candidates. Named member lookups report functions,
missing declarations, ambiguity or unavailable modules. Missing paths enter the input identity;
adding a declaration or package changes the next dependency snapshot. Duplicate bindings, shadowing,
ignored files, symlinks and syntax errors keep the report incomplete. Import aliases refer only to
static module bindings; arbitrary object aliases remain unsupported.

The admitted initialization model allows function declarations, static imports, comments and string expressions.
Packages, relative or dotted imports, wildcard imports, cyclic initialization and module effects retain cutoffs.
Custom search paths, import hooks, native modules and monkey patching remain outside the contract.
The model assumes the workspace root supplies its admitted modules. External rules remain caller-authored assumptions.
They cannot overlap any local function or import binding in the closure.

Budgets admit at most 16 modules, 256 KiB of module source and 128 import lookups.
The existing function, depth, transfer and response limits still apply. Incomplete results never enter the reuse cache.
Reuse validates every module and import candidate, configuration, rules and analyzer source identity.
It renews occurrences only inside that validated closure. Unrelated files can change without rerunning flow transfer.
The cache deliberately recomputes the whole closure after a relevant edit; it does not cache individual summaries.

`analysis.dependencies` exposes typed files, import candidates, resolutions and coverage.
`analysis.dependencies.dependency` supplies a captured `flow-inputs` dependency for a task step.
Its query reselects the named declaration in its exact root-local file on each resume.
It rechecks the current module closure, configuration and rule file before preserving satisfied evidence.
Missing or ambiguous entries invalidate captured dependencies. Independent steps retain their own evidence.
Rule files must live inside the workspace to create this plan dependency; analysis alone also permits explicit external rule files.

The [imported-flow acceptance](../tests/agent-eval/results/2026-09-25-imported-flow/result.json) records runtime and coordinate oracles,
negative lookups, dependency drift, restored plans and checked patch delivery. Three Lean theorems model import admission;
native tests compare all eight Boolean cases. These results do not prove Python source correspondence or parser correctness.

## Verified result reuse

`--inputs-only` reports the input identity without running flow transfer. It binds the defining file,
selection, rules, context, summary mode, budgets, analyzer implementation and indexed manifests/lockfiles.
The whole defining file covers helper bodies, local shadowing and negative same-file lookups.
`--imports` expands this scope to its static module closure and import candidates.
Incomplete analyses and skipped configuration snapshots cannot be reused. A changed input falls back to clean analysis.

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

`project identities` returns version 2 declaration identities with exact occurrences and lexical scopes.
Each identity separates the revision handle, declaration content digest and digest with the declaration name removed.
The SDK persists these records through the existing verified Merkle store. Its storage root identifies the whole snapshot;
it differs from a declaration content digest and a revision handle. Graph edges stay outside declaration content hashing.

Pass a captured page or SDK snapshot with `--from`; optional `--digest` binds its canonical JSON bytes.
The command searches indexed declarations within the current target. It combines three candidate rules:
identical declaration content, identical text except the declaration name, and the same path/scope/name/kind/language.
A renamed recursive function can remain missing because references inside its body also changed.
A changed original and its unchanged copy both remain visible. Multiple candidates remain ambiguous.
Several retained declarations competing for one current candidate also remain ambiguous, even with only one candidate each.
`matched` means one unshared syntactic candidate. It does not establish semantic equivalence.

`--limit` bounds rows to 1–500. `--candidates` bounds disclosed candidates per row to 1–64, with a default of 16.
`--bytes` bounds compact report bytes to 4,096–1,048,576, with a default of 32,768.
A row that cannot fit refuses and asks for a larger byte budget or fewer candidates.
Candidate counts and conflicts describe the full search; clipped candidate lists make the report incomplete.
Missing rows describe only the indexed declarations in the selected scope. Read scan coverage before reasoning about repository absence.
Only supplied old identities participate; a partial old page cannot produce a complete correspondence report.
Older identity reports lack the version 2 contract and require a fresh capture.

`fr_ir.correspondence` provides `IdentityPage`, `DeclarationIdentity`, `DeclarationSnapshot` and `CorrespondenceReport`.
Capture uses at most 64 pages and retains `complete=False` when the page budget ends.
Snapshots admit at most 1,000 identities and 1 MiB of canonical JSON.
`subset` explicitly selects disclosed handles; its completeness covers that selection only.
Indexed parameters and local declarations can appear beside functions. Select the intended handles explicitly.
`compare` validates page continuity, candidate reasons, conflicts and retained input identity.
`select(client, {old_handle: current_handle})` rechecks the whole comparison before returning fresh read targets.
It requires complete disclosure and distinct explicit choices. It never chooses the first row automatically.
Changed source or analyzer rules refuse a stale selection. Local record digests do not authenticate an untrusted producer.

`fr_ir.investigation_session.InvestigationSession` binds selected snapshots to plan steps.
Use `target_inputs(*source_paths)` to declare workspace and declaration-analyzer dependencies plus explicit source paths.
Capture the plan dependencies with `plan.resume(client)` before binding the target snapshots.
Session storage includes the plan and targets; reopening preserves their original revisions.
Resumption groups targets from the same revision to expose competing candidates across steps.
A session admits at most 64 target steps and 1,000 distinct target declarations.
The `declaration-analyzer` dependency invalidates evidence when correspondence rules change.

`resumed.refresh(client, step_id, choices, inputs=target_inputs("moved.py"))` requires a choice for every target of that step.
It captures the agent's fresh dependency declarations and clears the step's old evidence, action and structured action input.
It preserves acceptance requirements and independent satisfied evidence. Dependent stale steps need their own refresh.
Use fresh guide actions and immutable mutation reviews after refresh. Old reviews still refuse after a source change.
The session never executes an action or rewrites a retained review.

The [retained acceptance](../tests/agent-eval/results/2026-09-24-resumable-correspondence/result.json) covers six correspondence cases,
interrupted resumption, stale review refusal and fresh delivery with independent patch replay.
An independent Python AST oracle checks declaration coordinates. Four Lean theorems describe the candidate classification policy;
native tests compare sixteen cases. These checks do not prove semantic correspondence or host persistence.
Cold, warm and edited runs retain latency, context bytes and isolated peak RSS. They recompute correspondence on each call.

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

## Compiler observations

[Compiler evidence](project-checks.md#retained-compiler-diagnostics) adds Rust diagnostics from retained
reviewed checks. It binds command flags, source, workspace build configuration and declared external
identities. Typed pages preserve syntax/compiler differences and exact locations while exposing missing
mappings. Investigation plans retain these observations alongside the original passing or failing checks.

The [compiler task](../tests/agent-eval/compiler-evidence/task.json) pins a syntax-valid function that
fails under a strict build configuration. [Acceptance](../tests/agent-eval/results/2026-09-24-compiler-evidence/result.json)
retains real rustc and Cargo output, an independent six-case runtime oracle and stale-input refusals.
Coverage remains specific to the declared invocation. Compiler diagnostics do not prove runtime safety,
complete dependency discovery or implementation correspondence.

## Retained model proofs

Use `fr spec retain specs` to review a generated Lean package. Execute that selection with
`fr spec retain specs --run --basis DIGEST`. Store reviews and reports outside the scanned workspace.
The report binds all recognized workspace source files, local package files, checker executable bytes,
the running `fr` binary and the checker environment. Every run builds a fresh temporary package.
It then checks every Lean module directly, including modules absent from the default library imports.
The original package's build artifacts cannot supply retained evidence.

The initial contract accepts the exact `lakefile.toml` from `fr spec init`, its pinned Lean toolchain,
and an absent or empty dependency manifest. It refuses external packages, executable Lake configuration,
symlinks, more than 128 package files and more than 4 MiB of package content.
The report ceiling is 1 MiB. A build has 120 seconds; each module check has 30 seconds.
Execution retains bounded diagnostics and uses the check runner's process-group cleanup.
Tool discovery accepts direct Lean installations and elan-managed toolchains. It verifies the selected Lean version.
Executable identities use streaming hashes with a 2 GiB ceiling per file.
A changed input during execution prevents a passing report.

```python
from fr_ir.investigation import ProofRequirement, TaskPlan, TaskStep
from fr_ir.investigation_proofs import ProofReport, run_proofs

plan = TaskPlan("Establish the model property", ("model identity",), (
    TaskStep.proved("model", "Does Lean accept identity?",
        proofs=(ProofRequirement("specs", "FrSpecs/Model.lean", "identity"),),
        satisfies=("model identity",)),
))
reviewed = ProofReport.review(client, "specs")
result = run_proofs(plan, client, "model", reviewed, store)
assert result.passed
reopened = result.resumed.plan.resume(client)
```

The theorem name is the name in `/evidence/properties`; the module path is relative to the package.
A proof step retains explicit `required_proofs` and a `proof-inputs` dependency for each package.
The helper starts the step, executes its reviewed package, stores the report in verified Merkle objects,
and attaches the required theorems. Failed runs retain their reports and leave the step incomplete.
For multiple packages, execute each review and use `attach_proofs`; satisfy after the final attachment.
Each step accepts at most 64 distinct theorem requirements.

Resumption compares current input identities without running Lean. Source, package, toolchain or checker drift
invalidates the step and its dependents. Independent observations retain their own dependencies.
An unavailable package also invalidates a previously captured dependency. To retry, reset the step explicitly
and capture a fresh review. Keep the old proof report for comparison; it cannot authorize a mutation.
Fresh source edits still pass through ordinary immutable mutation review, checks, history and patch delivery.

A caller-supplied digest binds trusted retained output. It does not attest that a process ran.
Attachment rechecks module coverage, theorem declarations, assumptions, anchors, signature maps and debt.
The report states its trusted components and remaining obligations. Installed Lean libraries and host execution
remain trusted. Declared assumption discovery reports syntax; it does not compute transitive axiom dependencies.
A model theorem supplies no proof that source execution agrees with the model.
The finite Boolean fixture tests that correspondence only on its two admitted inputs.
