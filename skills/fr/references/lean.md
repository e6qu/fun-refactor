# Write and check a Lean proof

A theorem states the claim. Tactics are the proof steps you write for Lean to check.
Keep the supplied claim, model, assumptions and program unchanged.

## When the theorem already exists

A target joins the file and theorem with `::`, for example
`specs/FrSpecs/Rules.lean::access_rule`. Replace `<LEAN_TARGET>` below with the supplied
target. `spec goals specs` lists generated goals when the target is unknown.

```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```

Read the fixed claim, model and empty templates. Write your tactics, without a leading
`by`, in the extensionless file `.fr/proof-tactics`. You supply the proof steps.
The `--token-limit` value bounds serialized UTF-8 response bytes.

For a check that leaves source unchanged:

```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```

Read `passed` and Lean's diagnostics; a successful command exit can still report a failed
proof. Repair the tactics in that file. To check and write them into the proof region:

```sh
fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write
```

This command checks the proof and records its source change. Review the diff and deliver
the requested patch. If the request specifies `git diff --binary HEAD`, write that output
to `change.patch`. The separate source-edit workflow needs a saved edit plan; use actual
returned IDs when working with such a plan.

If you already edited the proof directly, check the owning Lean package and deliver the
requested patch. A contributing proof-task or proof-check step can help without creating
a source-edit transaction. Run supplied checks once and retain failed-check diagnostics.

## When a model or claim still needs to be created

Inspect `fr --json capabilities --lang lean`. A model describes selected source behavior
in Lean; proving its theorem does not establish a general source-to-model connection.

For typed pure source declarations, use `spec candidates src`, `spec property-task TARGET`,
then `spec plan TARGET --property-from FILE` and `spec scaffold --from PLAN --write`.
A property-task describes the model signature, allowed claim grammar and input digest.
Write the requested claim using that grammar, review the generated model and theorem,
then follow the proof steps above. Never substitute an easier claim.

Generated `Int` and `Nat` models use Lean arithmetic. Review source widths, overflow and
coercions before claiming correspondence. `spec verify specs` checks the owning package;
Lean and Lake must be installed. Hash renewal alone repairs no model or proof.

State the proved claim, domain, assumptions, axioms and trusted tools. A source hash
identifies bytes; a signature map identifies declared types; runtime examples check their
particular inputs. A checked model theorem leaves general implementation equivalence open.
