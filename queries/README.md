# Teach fr to recognize declarations and uses

These query files tell `fr` which syntax nodes declare names, use names or import other files.
A parser turns code into a syntax tree. A query matches parts of that tree, and a capture labels a match for the extractor.

For example, a Python file can declare `def greet(): ...` and later call `greet()`.
Capturing the declaration can make it appear in a symbol list. Capturing the call gives name resolution a use to connect to that declaration.
Test both before trusting a rename's affected-code list. Recognizing syntax alone does not establish safe edits.

## Add or change a pattern

The capture conventions at the top of [src/extract.rs](../src/extract.rs) define what the extractor reads.
Follow the [language development guide](../docs/development.md#add-or-extend-a-language) for registration and build steps.
Run builds and language tests on a dedicated runner under the project's resource rules.

When a grammar changes, inspect the upstream query files in [vendor](../vendor/README.md) and refresh their recorded copies.
Then check our patterns against the syntax the new parser produces.
A renamed syntax node can stop a pattern matching even when the project still builds.

Include a declaration, its ordinary caller and a nearby case that must stay distinct, such as a local name hiding an imported name.
Check the symbol list, the resolved callers and the reviewed rename diff.
Keep unsupported resolution explicit so a shorter caller list cannot silently become a claim of completeness.
