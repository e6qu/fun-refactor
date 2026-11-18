# Refactor and transform code with checked delivery

Start with the declaration and its consumers, preview the complete change, run the relevant checks,
then deliver a patch that another checkout can apply. A refactoring preserves behavior;
a translation or interface change must state the behavior and runtime contract it preserves.

The [roadmap](../PLAN.md#pr-2-complete-refactoring-and-transformations) keeps all advertised operation families in scope.
This inventory records their existing checks and the work being added in PR 2.
It does not treat a refused example as evidence that a promised operation is complete.

## Operation families

The maintained language matrix comes from [the capability registry](../src/capabilities.rs).
Use `fr capabilities --json` for the operation/language pairs in the installed build.
The [capability drivers](../tests/capability_claims.rs) exercise those claims; the tests below check their behavior.

| Family | Contract and checks |
| --- | --- |
| Rename | Preserve unrelated bindings and import aliases; reject capture. [Scope](../tests/rename_scope.rs), [capture](../tests/rename_capture.rs), [aliases](../tests/rename_import_aliases.rs), [dispatch families](../tests/rename_hierarchy.rs), [reversal](../tests/rename_inverse.rs) |
| Delete | Refuse declarations with remaining consumers. [Deletion](../tests/refactor_delete.rs), [method families](../tests/signature_hierarchy.rs) |
| Signature changes | Update declarations and callers together; preserve binding and evaluation. [Evaluation and binding](../tests/signature_safety.rs), [Java](../tests/signature_java.rs), [shell](../tests/signature_bash.rs), [Terraform and Sass](../tests/signature_config.rs) |
| Extract and inline | Preserve scope, expression precedence and evaluation counts. [Extraction effects](../tests/extract_effects.rs), [function extraction](../tests/extract_function.rs), [inline scope](../tests/inline_scope.rs), [calls](../tests/inline_call.rs), [precedence](../tests/inline_call_precedence.rs) |
| Restructure and rewrite | Preserve identity, evaluation and untouched members. [Identity](../tests/restructure_identity.rs), [languages](../tests/restructure_languages.rs), [members](../tests/restructure_members.rs), [meaning](../tests/rewrite_meaning.rs), [compiler checks](../tests/rewrites_compile.rs) |
| Imports and moves | Keep dependencies reachable and imports consistent. [Imports](../tests/refactor_imports.rs), [move dependencies](../tests/move_dependencies.rs), [language moves](../tests/move_languages.rs), [crate boundaries](../tests/move_out_of_the_crate.rs) |
| Flag removal | Apply the declared constant through supported branches. [Cascade implementation and tests](../src/refactor/cascade.rs), [capability drivers](../tests/capability_claims.rs) |
| Authoring and recipes | Preview coordinated edits; refuse overlap and partial failure. [Author batches](../tests/author_cli.rs), [recipes](../tests/recipe.rs), [checked structural delivery](../sdk/python/tests/test_structural_change.py) |
| Translation | Retain the admitted arithmetic, evaluation, type and runtime rules. [Corpus](../tests/translate_corpus_sweep.rs), [projects](../tests/translate_projects.rs), [division](../tests/translate_integer_division.rs), [evaluation order](../tests/translate_precedence.rs), [exceptions](../tests/translate_exceptions.rs) |
| Configuration, markup and cross-language edits | Follow declared links without inventing dynamic relationships. [Cross-language cases](../tests/cross_language.rs), [language stitching](../tests/stitch_languages.rs), [configuration signatures](../tests/signature_config.rs) |
| Framework and application transformations | Preserve the admitted route and adapter contracts. [Migration commands](../tests/migration_cli.rs), [runtime checks](../tests/migration_runtime.rs), [Next.js routes](../tests/translate_nextjs_routes.rs) |
| Transaction and delivery | Refuse stale plans; support exact reversal, replay and recovery. [History](../tests/history_cli.rs), [patch replay](../tests/history_patch_cli.rs), [interrupted writes](../tests/host_recovery.rs), [structural delivery](../sdk/python/tests/test_structural_change.py) |

These checks support the declared forms of each operation. They do not establish arbitrary-program equivalence,
unrestricted dynamic dispatch or whole-project migration outside an existing contract.
External consumers remain an explicit migration obligation.

## Signature changes being completed

An unused parameter does not imply that its argument is safe to discard.
For example, deleting the second parameter from `calculate(price, record_purchase())` would remove the call to `record_purchase`.
The shared planner now refuses that edit before producing a transaction.
The same rule reaches the general command, recipes and checked authoring.
A recipe that first renames a function and then attempts an unsafe parameter removal must write neither change.

Moving a parameter shifts the intervening positions. Moving 0 to 2 changes `(a, b, c)` to `(b, c, a)`.
Every argument in that range must have established simple evaluation; moving across a call can change its relation to a retained read.
Shell positional reads are renumbered with their call sites, and quoted command or arithmetic substitutions cannot be discarded or reordered.

Python removal selects the supplied value by its actual binding, including mixed positional and keyword calls.
Omitted defaults stay omitted. A move that supplies only part of the moved range refuses.
Python and TypeScript defaults whose evaluation could change also refuse, as do unchecked Python annotation effects.
Variadic, grouped and keyword-only signatures require a separate binding rule and remain explicit refusals.

The [structural-change guide](structural-changes.md) describes the narrower checked-authoring contract.
It retains primitive Rust signature requirements and requires independent compiler and behavior checks.
An explicit documentation-preservation policy leaves comments unchanged during a rename;
executable strings still need their references resolved before checked delivery.

## Completion record

PR #621 supplied the graph and analysis foundations. PR 2 is in progress.
Its focused hosted workflow checks signature behavior, recipes, the refactoring unit tests and SDK delivery.
The full native gate remains responsible for all operation/language claims and the transformation suites.
Current evidence and documentation audits must pass on the final revision before this work is marked complete.
Existing M2 outcomes and original evaluation limits remain unchanged.
