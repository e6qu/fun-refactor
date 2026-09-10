# Export a patch or inspect Git

```sh
fr git status --limit 8
fr git diff app.py --limit 8
fr history patch '<TX>' --output '<PATCH>'
fr history patch '<TX>' --git-check --index --against '<RECEIVER>'
```
Write `<PATCH>` outside the project. Export refuses replacement and returns its digest and size. `<RECEIVER>` must match the transaction's starting source and index; `--reverse` exports the inverse. These commands do not stage or commit.

Text patches cover text, symlinks, and executable-mode projection. Source history works without Git. For staging, commits, or worktrees read [Git administration](git-admin.md).
