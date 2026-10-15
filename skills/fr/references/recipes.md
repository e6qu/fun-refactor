# Compose related refactorings

A recipe orders edits. Its selector chooses code; its expectation checks a declared result. Preview the combined cleanup before writing.

Inspect supported steps:

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

Preview before the single rename changes source. Set expectations for your task; this example changes two files.
`--save-plan` saves a transaction for history. Each step sees earlier proposed edits; failed expectations prevent writing.
A recipe uses existing operations and retains their support limits.
