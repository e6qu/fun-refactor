# Export a patch or inspect Git

```sh
fr git status --limit 8
fr git diff app.py --limit 8
fr history patch '<TX>' --output '<PATCH>'
fr history patch '<TX>' --git-check --index --against '<RECEIVER>'
```

Write `<PATCH>` outside the project. Export refuses an existing file and returns its digest and size without echoing it. `<RECEIVER>` must match the transaction's recorded source and index; `--reverse` exports the inverse. Inspection, export, and checks do not stage or commit.

Text patches cover regular text, symlinks, and executable-mode projection, with explicit limits. Source history works without Git. For staging, commits, or worktrees read [Git administration](git-admin.md).
