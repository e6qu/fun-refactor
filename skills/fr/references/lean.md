# Write and check a Lean proof

A theorem states the claim. Tactics are the proof steps you write for Lean to check.
Keep the supplied claim, model, assumptions and program unchanged.

## When the theorem already exists

A target names the file and theorem. Run `spec goals specs` and copy its `target`
as `<LEAN_TARGET>`.

```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```

Read the fixed claim, model and empty templates. Write a draft early, without a leading
`by`, in the extensionless file `.fr/proof-tactics`. Check it and use the diagnostics to
repair it. Keep relative indentation; fr adds two spaces to each line. You supply the proof steps.
The `--token-limit` value bounds serialized UTF-8 response bytes.

`refine ?_` opens a temporary goal. Later tactics must solve it; an unfinished goal fails.

Check before searching the library; this leaves source unchanged:

```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```

Read `passed` and Lean's diagnostics; a successful command exit can still report a failed
proof. Repair the tactics in that file. If diagnostics require a library definition,
search its type's folder rather than indexing the whole installation.

For library lookup, the pinned Lean binary's `--print-prefix` locates its installation.
Under `src/lean/Init/Data`, choose the folder named by a type in your goal.
A namespace groups definitions: `List.length` belongs to `List`. Search for `length`,
the part after the dot.
Use that path as `<LEAN_LIBRARY_DIR>` and a name fragment from the model as `<LIBRARY_QUERY>`:

```sh
fr -C '<LEAN_LIBRARY_DIR>' project find '<LIBRARY_QUERY>' --contains --signature --source --bytes 1536 --limit 12
```

`--contains` matches a literal name fragment. Read the returned headers for argument types
and order. `--bytes` limits source text across the page; metadata adds output.
Check coverage and remaining results; refine the fragment or follow the cursor.

To check and write the tactics into the proof region:

```sh
fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write
```

This command checks the proof and records its source change. Review the diff and deliver
the requested patch. If the request specifies `git diff --binary HEAD`, write that output
to `change.patch`. The separate source-edit workflow needs a saved edit plan; use actual
returned IDs when working with such a plan.

These commands also work after a direct edit removed the unfinished-proof marker.
Keep the named proof-region markers. Run supplied package checks once, retain failed-check
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
