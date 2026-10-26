# Help the selected agents write three small Lean proofs

M3 asks whether Codex with Luna Fast and OpenCode2 with GLM Flash or non-Pro Kimi K3
can write and check proofs for fixed program models. A model describes selected behavior
in Lean; a theorem states a claim about that model. The agent supplies the proof steps,
called tactics. It must keep the program, model, theorem and assumptions unchanged.

This milestone is in progress. Scripted controls and the real agent comparison are separate.
The complete implementation, examples, checked attempts and tracking belong in one PR.
The [roadmap](../../../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs)
owns its completion conditions.

## The three requests

| Request | Exact claim | Example |
| --- | --- | --- |
| Access | The model returns true exactly when `staff` or `owner` is true | A non-staff owner is accepted; a non-owner without staff access is rejected |
| Page count | Invalid inputs return -1; valid signed-64 inputs produce a result between zero and the item count; 11 items at 5 per page produce 3 | Ten items fit on two pages of five; eleven need three |
| Range limit | The model accepts exactly when `offset + count <= total` | An empty range at the end fits; a range past the end does not |

See the exact [source files](fixtures/src), [fixed requirements and limits](tasks.json),
[case names and targets](cases.json), and [checker pins and allowed axioms](checkers.json).
An axiom is an assumption Lean may use without a proof. The checks permit only the listed
standard Lean axioms and reject new assumptions introduced by a candidate.

The page-count source is the existing [translation example](../checked-translation/pages.rs).
Its signed-64 inputs range from -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807.
The Lean model uses unbounded mathematical integers. `Int.tdiv` divides by truncating
toward zero; `Int.tmod` returns the corresponding remainder. These match Rust on admitted inputs. For example, -7 / 3
truncates to -2; Lean's ordinary integer `/` instead rounds down to -3.
This particular source rejects negative item counts and nonpositive page sizes before division.
Its theorem includes the result bounds and the rounding example; it is not a full division specification.

The range source uses unsigned-64 inputs from zero through 18,446,744,073,709,551,615.
It checks `offset <= total` before evaluating `count <= total - offset`, so subtraction
cannot wrap. Its model uses Lean `Nat`, the unbounded nonnegative integers. The theorem
holds for every `Nat` input, which includes the unsigned-64 input domain. Separate Rust
checks use Rust's 128-bit unsigned type, `u128`, to compute the expected answer at the boundaries.
That type can hold the sum of any two unsigned-64 values without overflow.

## Check the complete examples before calling models

Start with the [access-rule lesson](../../../docs/first-lean-proof.md). It supplies the complete
program, claim, bad proof, diagnostics, corrected tactics and checked write. CI executes its
actual snippets. Supplied lesson tactics demonstrate the commands; they are not an agent result.

The same Boolean rule has a separate source and Lean proof check for each language:

| Case | Source | Selected declaration |
| --- | --- | --- |
| Rust | [allowed.rs](fixtures/src/allowed.rs) | `allowed` |
| Python | [allowed.py](fixtures/src/allowed.py) | `allowed` |
| Go | [allowed.go](fixtures/src/allowed.go) | `allowed` |
| Java | [Allowed.java](fixtures/src/Allowed.java) | `Allowed::allowed` |
| TypeScript | [allowed.ts](fixtures/src/allowed.ts) | `allowed` |
| TSX | [allowed.tsx](fixtures/src/allowed.tsx) | `allowed` |
| Zig | [allowed.zig](fixtures/src/allowed.zig) | `allowed` |
| Lean | [allowed.lean](fixtures/src/allowed.lean) | `allowed` |

These functions declare their input and result types and have no external reads or writes.
The TSX case is an ordinary typed helper, with no JSX rendering claim. Other parsed forms
do not automatically become supported proof cases.

The **M3 proof writing** GitHub workflow prepares each case, generates its model and fixed
claim, reveals one goal, and checks supplied controller tactics. It also executes each source
independently: all four Boolean pairs, page-count boundary cases and range boundary cases.
The workflow requires failed-tactic diagnostics, rejected unfinished proofs, unchanged protected
inputs, a checked write, clean patch delivery and exact undo/redo. Regeneration must retain
an unrelated handwritten theorem. Changing a source comparison, model, claim or imported
local definition must expire the previous proof record; a false claim must fail checking.

Before a paid call, all ten cases, six scripted native-client exchanges and eighteen isolated
positive/negative patch controls must pass. The isolated checker runs without network access
or runner credentials. It rejects missing patches, malformed patches, unfinished proofs,
incorrect tactics and changed claims; it accepts and rechecks the supplied reference patch.
The control receipt identifies the exact checker image, source, scripts and task inputs.
Changing a native source input requires a rebuilt and tested executable. Reusing a specifically
named hosted build is allowed only after those native inputs match exactly.

Controller proofs live in [controls](controls), outside every live agent project. Prepared
source archives contain unfinished proofs only. Do not copy supplied reference tactics into
an agent project or present a reference proof as unaided agent work.

## Run and measure the selected local CLIs

Use the [selected client guide](../selected-agents/README.md) for client identities and the
[local runner](../../../tools/agent_eval/selected_local.py) with the explicit `M3` milestone.
Build the macOS fr executable on GitHub. Freeze the downloaded source archives, successful
controls, checker image, CLI releases, model settings and instructions before the first call.
Use the workstation resource guard for every local invocation. Do not rebuild fr locally.

Three profiles × three Rust tasks × two tool setups × two repetitions gives **36 attempts**.
The baseline is the same app and model using ordinary reading, search, editing and shell tools.
The other setup adds fr and the project skill. Both get identical source, models, fixed claims,
checking commands and a 150-second complete-attempt deadline, with time reserved for delivery.
The model can repair rejected tactics within that attempt. A failed slot is never retried.

Record complete elapsed time, available CLI token categories, timeouts, submitted patches and
reviewed fr contributions. Keep missing counters and account charges unknown. These are proof-writing
times, not execution-speed benchmarks of Rust or Lean. The comparison cannot establish general
time or money savings. Each profile needs an independently accepted fr proof for each task,
with a useful fr step identified, within the frozen repetitions.

Only submitted patches and summaries may be published. Raw prompts, CLI transcripts, session
exports, local paths and account data stay local. The public exporter selects approved fields;
GitHub independently checks every published patch against its untouched original source.
A valid patch from a timed-out attempt remains a failed attempt.

## State the proof boundary

A checked proof establishes its theorem under the supplied Lean definitions and assumptions.
A source hash identifies bytes; matching signatures identify declared types; finite executions
compare particular input cases. None of those alone proves the general source-to-model connection.
Every result retains `source_implementation_proved: false`.

The recorded trusted components include fr's parser and translation, its hashing and file handling,
the pinned Lean checker and standard axioms, source compilers and the execution environment.
The comparison freezes the local checking binaries and records hosted tool versions. Package
fingerprints cover collected local Lean sources and build metadata; external dependency contents
remain trusted. See [proof receipts and limits](../../../docs/agent-formalization.md#verification-boundary).
