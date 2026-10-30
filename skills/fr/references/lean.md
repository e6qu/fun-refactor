# Write and check a Lean proof

A theorem states the claim. Tactics are the proof steps you write for Lean to check.
Keep the supplied claim, model, assumptions and program unchanged.

## When the theorem already exists

Copy `spec goals specs`'s Lean `target` as `<LEAN_TARGET>`.

```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```

Read `claim.parameters` and `claim.parts`: each part lists assumptions and conclusions.
All assumptions apply to that part; all its conclusions need proofs. Alternatives stay together.
Draft tactics without `by`; fr adds two spaces per line. Keep relative indentation.
The `--token-limit` value bounds serialized UTF-8 response bytes.

`refine ?_` opens a temporary goal. Solve it before delivery.

Check before searching the library; this leaves source unchanged:

```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```

Read `passed` and Lean's diagnostics; a successful command exit can still report a failed
proof. Repair errors and warnings in that file. If diagnostics require a library definition,
search its type's folder rather than indexing the whole installation.
For Boolean comparisons and division arguments, read [Arithmetic](lean-arithmetic.md).
`draft_location` names your tactics file, line, column and source excerpt.
`checked_location` identifies Lean's checked file; it may have extra generated lines.
`context` retains the goal (`⊢`); `context_omitted_lines` counts omitted lines.

Run `lean --print-prefix`; under `src/lean/Init/Data`, choose the type's folder.
A namespace groups names: search `length` for `List.length`.
`<LEAN_LIBRARY_DIR>` is that folder; `<LIBRARY_QUERY>` is the name fragment:

```sh
fr -C '<LEAN_LIBRARY_DIR>' project find '<LIBRARY_QUERY>' --contains --signature --limit 4
```

Read the returned argument types and order. `--contains` matches a literal name fragment;
the command returns at most four declarations without proof bodies.
Check coverage and remaining results; refine the fragment or follow the cursor.
Syntax gaps mean unread code. An empty result with gaps cannot establish absence.

To check and write the tactics into the proof region:

```sh
fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write
```

Review the saved diff. If asked for `git diff --binary HEAD`, write it to `change.patch`.

Keep proof-region markers and `.fr/checks.json`. Run supplied package checks,
retain failed-check diagnostics and deliver the requested patch.

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

State the claim, domain, assumptions, axioms and trusted tools. Hashes identify bytes;
signatures identify types; runtime examples cover tested inputs. A model proof leaves
source/model equivalence open.
