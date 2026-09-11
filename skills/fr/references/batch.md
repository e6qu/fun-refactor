# Batch broad project queries

Save this manifest outside the source tree, substituting paths and exact names needed by the task:

```json
{"schema":"fr-project-batch-1","requests":[{"id":"structure","arguments":["map","app.py","--depth","2","--limit","12"]},{"id":"lookup","arguments":["find","greet","--signature"]},{"id":"source","arguments":["show",{"request":"lookup","pointer":"/rows/0/0"},"--source","--bytes","256"]},{"id":"callers","arguments":["calls",{"request":"lookup","pointer":"/rows/0/0"},"--direction","incoming","--limit","8"]},{"id":"tests","arguments":["tests","app.py","--limit","8"]},{"id":"gaps","arguments":["gaps","--limit","8"]}]}
```

```sh
fr project batch --from '<PROJECT_QUERIES>' --report-bytes 8192
```

Read each request status. The outer revision, handle prefix, coverage and context basis apply to
every nested report. A nested report omits those common fields. `omitted-report-budget` means that
complete report did not fit; use its ordinary subcommand when it is still needed.

A reference argument uses an RFC 6901 pointer into an earlier report and must resolve to a string.
It works even when the earlier report is omitted from output. Keep references backward-only and use
returned column positions: `find` puts its handle in column 0, while `select` puts it in column 1.
