# Git patches and separate Git operations

Git is optional for ordinary exploration and source history.
Inspect repository state before an operation whose behavior depends on the index or HEAD:

```sh
fr git status --limit 8
fr git diff app.py --limit 8
fr history patch '<TX>' --json
fr history patch '<TX>' --git-check --index --against '<RECEIVER>'
```

`<TX>` is a source-history transaction. Set `<RECEIVER>` to a separate Git worktree whose working files and index match the recorded starting state.
Use `--reverse` to check the reverse patch against the recorded result.
The JSON export carries the patch string; write that string to the chosen artifact file when a patch is requested.
Exporting or checking a patch does not stage or commit it.
Git text patches preserve their documented executable-mode projection, not every Unix permission bit or arbitrary binary content.

For staging, commits or worktree lifecycle operations, load [Git administration](git-admin.md).
