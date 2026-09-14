# Export a patch or inspect Git

```sh
fr git status --limit 8
fr git diff app.py --limit 8
fr git diff app.py --calls --workspace-context --depth 2 --limit 8
fr history patch '<TX>' --output '<PATCH>'
fr history patch '<TX>' --git-check --index --against '<RECEIVER>'
```
Write `<PATCH>` outside the project. `<RECEIVER>` must match recorded source and index. `--reverse` selects the inverse.

Workspace context emits reachable call metadata without source. Use `--include FILE` beyond 256 files or 64 MiB.

For staging, commits or worktrees read [Git administration](git-admin.md).
