# Compose related refactorings

Use a recipe when a repeated cleanup needs several related edits. Review the combined preview before writing any of them.

Load the vocabulary when authoring:

```sh
fr --json recipe --vocabulary
fr --json recipe rename.recipe --explain
fr --json recipe rename.recipe
```

For the greeting example, `rename.recipe` contains:

```recipe
schema 1
recipe rename-greeting {
  requires symbol "greet" where kind=function in="app.py"
  rename to "welcome" where kind=function name="greet" in="app.py"
  expect matched = 1
  expect changed = 2 files
  expect refusals = 0
}
```

Preview this alternative before the single rename changes the source.
Set expectations for the actual task; the example touches two files.
Save a recipe with `--save-plan` and apply its returned transaction through history.
All steps see the preceding virtual result, and failed expectations prevent source writes.
A recipe composes existing operations; it cannot authorize an unsupported transformation.
