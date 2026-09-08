# Export a patch or inspect Git

Git is optional for source history. Inspect state before actions that depend on the index or HEAD:

```sh
fr git status --limit 8
fr git diff app.py --limit 8
fr history patch '<TX>' --output '<PATCH>'
fr history patch '<TX>' --git-check --index --against '<RECEIVER>'
```

`<TX>` is a source transaction. Write `<PATCH>` outside the project; output mode refuses an existing file and returns its SHA-256 and byte count without repeating its contents. `<RECEIVER>` is a separate worktree matching the transaction's recorded start, including the index. `--reverse` selects the opposite patch. Export and checks do not stage or commit.

Text patches preserve documented executable-mode projection, not arbitrary binary content or every permission bit. For staging, commits, or worktrees load [Git administration](git-admin.md).
