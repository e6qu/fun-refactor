# Prove a small access rule with Lean

Suppose your program lets staff members and owners open a document.
We will generate a Lean model of that function, state that exact rule, reject a bad proof,
and save a checked proof. All files and commands are included below.

Lean checks proofs. A **model** describes the selected program behavior in Lean.
A **theorem** states the claim to prove; **tactics** are instructions that construct its proof.
Lake builds a package of Lean files. A checked theorem about a model does not by itself prove
that the original Rust program behaves the same way. We will keep that connection open.

You need fr, Python 3 and Lean/Lake 4.28.0. The generated package pins that Lean release.
See [installation](../README.md#install) and the [workstation rules](development.md#working-on-a-shared-desktop).
Use a runner for builds and full checks on a shared desktop. This lesson makes no model calls;
its supplied proof steps are a worked example, not an agent success result.

## Create the program

Open a new empty folder and create a `src` subfolder. Save this project description as `Cargo.toml`:

```toml
[package]
name = "fr_first_proof"
version = "0.0.0"
edition = "2024"

[workspace]
```

Save this complete function as `src/lib.rs`:

```rust
pub fn allowed(staff: bool, owner: bool) -> bool {
    if staff {
        return true;
    }
    owner
}
```

A Boolean is either `true` or `false`. The function immediately accepts staff members;
otherwise it returns whether the person owns the document. There are four input pairs:

| Staff | Owner | Access |
| --- | --- | --- |
| false | false | false |
| false | true | true |
| true | false | true |
| true | true | true |

## State the exact claim

Run these commands from the example folder:

```sh
fr -C . --json spec init specs
fr -C . --json spec init specs --write
fr -C . --json spec property-task src/lib.rs::allowed > property-task.json
```

`-C .` selects this folder. JSON is structured output that another program can read.
`init` previews the Lean package; `--write` creates it. Here we choose `specs` as its directory.
A dependency is another library the package uses. Its lockfile records those choices before
the first proof task; this example starts with no dependencies.
`property-task` describes the selected model, allowed claim forms and a fingerprint of those inputs.
A fingerprint identifies particular content; it is not a proof of correctness.

We want equality with `staff || owner`, meaning “staff or owner.”
Save the following as `make-property.py`. It reads the fingerprint from the actual task
and writes our claim as a small structured tree. `variable` names an input, `model` calls
the selected generated model, `binary` applies an operation, and `equals` asks for equality.
The two named parameters are Boolean inputs to the theorem, covering every value rather than one example.

```python
import json
from pathlib import Path

task = json.loads(Path("property-task.json").read_text())
staff = {"kind": "variable", "name": "staff"}
owner = {"kind": "variable", "name": "owner"}
claim = {
    "schema": "fr-formal-property-1",
    "task_digest": task["object_digest"],
    "name": "access_rule",
    "parameters": [
        {"name": "staff", "lean_type": "Bool"},
        {"name": "owner", "lean_type": "Bool"},
    ],
    "proposition": {
        "kind": "equals",
        "left": {"kind": "model", "arguments": [staff, owner]},
        "right": {"kind": "binary", "operator": "or", "left": staff, "right": owner},
    },
}
Path("property.json").write_text(json.dumps(claim, indent=2) + "\n")
```

```sh
python3 make-property.py
fr -C . --json spec plan src/lib.rs::allowed --property-from property.json > formal-plan.json
fr -C . --json spec scaffold --from formal-plan.json --package specs
fr -C . --json spec scaffold --from formal-plan.json --package specs --write
fr -C . --json proof list specs --bytes 4096 --limit 8
fr -C . --json proof show specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --bytes 4096
```

The plan describes the selected source, generated definition, claim and assumptions. Review it before
scaffolding, which creates the Lean definition and an unfinished theorem. The goals report lists the
one claim. `proof show` reveals its context, parameters and the claim's parts. An unfinished theorem contains
`sorry`, a visible proof gap; it is not an accepted proof.

Each `claim.parts` entry lists `assumptions` and `conclusions` copied from the theorem's syntax.
Assumptions are facts you may use while proving that part. Conclusions are statements you must prove.
An empty assumptions list means that part needs no extra assumption beyond the theorem's parameters.
For example, a page-count claim lists “items is nonnegative” and “page size is at least one” separately.
An alternative such as “items is negative or page size is nonpositive” stays together; it does not assert both.
This view supplies no proof steps and does not change the theorem. Lean still checks the proof's meaning.

For numeric proofs, the [arithmetic examples](../skills/fr/references/lean-arithmetic.md)
introduce Boolean comparisons, assumptions joined by “and” or “or”, and library facts
whose arguments are proofs. Each example is checked with the pinned Lean release.

For these exact names, the theorem target is
`specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule`.
The text before `::` names the file; the text after it names the theorem.
For different source names, use the returned target rather than guessing it.
Keep this claim and the generated definition fixed while repairing proof steps.

## Read a rejection, then check the proof

Save this deliberately wrong tactic as `bad-proof.lean`:

```lean
exact false
```

```sh
fr -C . --json proof check specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft bad-proof.lean --bytes 4096 --timeout-seconds 30
```

The report should say `passed: false` and include Lean's explanation: a Boolean value is not a proof
of the requested equality. A successful process exit means a report was produced; inspect `passed`
to find out whether the proof worked. A rejected attempt has no checked receipt and changes no source.

Each diagnostic's `draft_location` points into your tactics file and includes a short source excerpt.
For example, an unknown tactic on line 2 of `bad-proof.lean` points to that file's line 2.
`checked_location` identifies Lean's checked file, whose generated lines can shift the error's position.
A location is `null` when the error cannot be assigned to that file, such as an error in an imported module.
`context` includes the goal marked by `⊢`; `context_omitted_lines` counts lines left out of the bounded report.

## Inspect your progress after each step

You can ask Lean where you are before the whole proof is finished.
Save `initial.lean` with this one tactic, which deliberately does nothing:

```lean
skip
```

```sh
fr -C . --json proof state specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft initial.lean --at 1:5 --bytes 4096 --timeout-seconds 30
```

`--at 1:5` means line 1, character column 5: immediately after `skip`.
Lines and columns start at one. Count Unicode characters rather than bytes.
The report identifies the original `theorem`, the checked draft fingerprint and `draft_location`.
In `goals`, lines such as `staff owner : Bool` are available inputs.
The statement after `⊢` is the current objective: prove the access rule.
The parameters are already available; introducing `staff` again would be a mistake.

Now save `partial.lean`:

```lean
cases staff
· rfl
· skip
```

`cases staff` splits the proof into false and true cases. Each bullet `·` focuses one case.
`rfl` proves equality when both expressions reduce to the same value.
Here the false case reduces to `owner = owner`; the true case reduces to `true = true`.
We leave the second case open deliberately. Inspect three positions:

```sh
fr -C . --json proof state specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft partial.lean --at 1:12 --bytes 4096 --timeout-seconds 30
fr -C . --json proof state specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft partial.lean --at 2:6 --bytes 4096 --timeout-seconds 30
fr -C . --json proof state specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft partial.lean --at 3:7 --bytes 4096 --timeout-seconds 30
fr -C . --json proof check specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft partial.lean --bytes 4096 --timeout-seconds 30
```

After the split, `status: "goals"` includes both cases and their available facts.
After the first `rfl`, `closed-at-position` means that branch is finished.
At the final `skip`, one goal remains. The complete check reports `passed: false`.
This is why an empty local goal list is never a proof receipt.
A cursor outside a tactic reports `no-tactic-at-position`; move into a tactic to inspect progress.
Every inspection has `acceptance: "not-checked"`.

The selected `--bytes` bounds UTF-8 response bytes, not model tokens. If the complete
state does not fit, fr refuses rather than silently removing a goal or assumption.
The explicit 30-second deadline covers dependency preparation and the editor query together.
Inspect after each new step and use its actual goal to choose the next one.

Replace the remaining `skip` with `rfl`, and save the complete tactics as `proof.lean`:

```lean
cases staff
· rfl
· rfl
```

The tactics file omits `by`: fr inserts these steps inside the existing proof region.
Only a complete check with `passed: true` establishes acceptance. Then check and save:

```sh
fr -C . --json proof check specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft proof.lean --bytes 4096 --timeout-seconds 30
fr -C . --json proof save specs/FrSpecs/SrcLibRsAllowed.lean::allowedModel_access_rule --draft proof.lean --bytes 4096 --timeout-seconds 30
fr -C . --json spec check specs --strict --max-debt 0
fr -C . --json spec evidence specs
```

The corrected check must report `passed: true`. It leaves source unchanged and returns a receipt,
which identifies this checked goal, proof, checker and package inputs. `proof save` checks
the complete draft, inserts the proof and records an undoable change. Other source and
proof regions retain their bytes. The strict check requires current source/signature references and
zero unfinished proof gaps. `evidence` builds the owning package and reports its claims and assumptions.

## What the result establishes

Lean accepted that this model allows exactly staff members or owners, for every Boolean input pair.
The lesson supplied its proof. An agent independently writing tactics is a separate result.

The source fingerprint identifies the reviewed Rust declaration. It does not prove the translation
from Rust into Lean. `correspondence.proved_implementation_model` remains `false`; keep that limit
beside the checked theorem. Finite source execution tests can give additional examples, but cannot
replace a proof of the connection.

Changing the source, model, property or relevant local package inputs requires a fresh reviewed task
and check. A changed dependency is rebuilt before checking tactics. Do not renew a fingerprint to
make an old proof appear current. Regenerating supported definitions preserves both declared proof
regions and the separate handwritten extension region.

See [proof results and limits](lean-specs.md), [agent proof authoring](agent-formalization.md), and
the [concrete milestone](../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs).
