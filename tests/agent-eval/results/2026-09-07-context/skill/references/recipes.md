# Compose related refactorings

Load the recipe vocabulary only when authoring a recipe:

```sh
fr --json recipe --vocabulary
fr --json recipe rename.recipe --explain
fr --json recipe rename.recipe
```

For the two-file greeting example, `rename.recipe` contains:

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

This is an alternative to the single rename above; preview it before that rename changes the source.
Choose expectations from the task's actual scope, rather than copying the example's file count.
Save a recipe with `--save-plan` and apply its returned transaction through history.
All steps see the preceding virtual result, and failed expectations prevent source writes.
A recipe composes existing operations; it cannot authorize an unsupported transformation.

