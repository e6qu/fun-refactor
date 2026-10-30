# Write and check a Lean proof

A theorem states the claim. Tactics are the proof steps you write for Lean to check.
Keep the supplied claim, model, assumptions and program unchanged.

## When the theorem already exists

A target names the file and theorem. Copy `spec goals specs`'s `target` as `<LEAN_TARGET>`.

```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```

Read `claim.parameters` and `claim.parts`: each part lists assumptions and conclusions.
All assumptions apply to that part; all its conclusions need proofs. Alternatives stay together.
Write proof steps early in `.fr/proof-tactics`, without `by`.
Keep relative indentation; fr adds two spaces to each line.
The `--token-limit` value bounds serialized UTF-8 response bytes.

`refine ?_` opens a temporary goal. Solve it before delivery.

Check before searching the library; this leaves source unchanged:

```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```

Read `passed` and Lean's diagnostics; a successful command exit can still report a failed
proof. Repair the tactics in that file. If diagnostics require a library definition,
search its type's folder rather than indexing the whole installation.
`draft_location` names your tactics file, line, column and source excerpt.
`checked_location` identifies Lean's checked file; it may have extra generated lines.
`context` retains the goal (`⊢`); `context_omitted_lines` counts omitted lines.

For library lookup, `lean --print-prefix` locates the installation.
Under `src/lean/Init/Data`, choose the type's folder.
A namespace groups names: search for `length` from `List.length`.
Use that path as `<LEAN_LIBRARY_DIR>` and a name fragment from the model as `<LIBRARY_QUERY>`:

```sh
fr -C '<LEAN_LIBRARY_DIR>' project find '<LIBRARY_QUERY>' --contains --signature --limit 4
```

`--contains` matches a literal name fragment. Read headers for argument types and order.
This returns at most four declarations without proof bodies. Try the actual argument types in your draft.
Check coverage and remaining results; refine the fragment or follow the cursor.
Syntax gaps mean the reader missed some code. An empty result with gaps does not show
that a library fact is absent. Keep the gap visible when reporting the search.

To check and write the tactics into the proof region:

```sh
fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write
```

This command checks and saves the proof. Review the diff and deliver the requested patch.
If asked for `git diff --binary HEAD`, write that output to `change.patch`.
The separate source-edit workflow needs a saved edit plan; use returned IDs.

`prove` also accepts a directly edited proof.
Keep the named proof-region markers and `.fr/checks.json`. Run supplied package checks once, retain failed-check
diagnostics and deliver the requested patch.

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
