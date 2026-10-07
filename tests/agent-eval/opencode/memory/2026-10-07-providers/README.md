# Provider metadata preserved within the memory budget

[Hosted run 37578206297](https://github.com/e6qu/fun-refactor/actions/runs/37578206297)
tested the exact 1,309-byte public catalog. All three candidates on each platform
passed the 640 MiB admission target: 475–483 MiB on Linux and 548–571 MiB on macOS.
The default macOS controls still hit the unchanged 768 MiB limit.

The isolated endpoint check also passed on both platforms. The empty catalog
lost both endpoints; the small catalog resolved each endpoint and adapter,
preserved reasoning and tool capabilities, and retained the configured 32,768
context and 2,048 output limits. It used fixture authentication only and made no
model request.

After hosted admission, one guarded workstation source-delivery control passed
at 550 MiB, 2.15 CPU seconds and 4.6 seconds elapsed. A separate bounded check
through the configured client confirmed both endpoint/adapter matches and model
limits. It retained only those matches, model identifiers and process counters.

`manifest.json` binds the frozen scripts and archives. Hosted archives contain
all memory captures and endpoint-check stdout, stderr, catalogs, counters and
results. Disposable endpoint-check XDG databases and caches are omitted. The
workstation archive contains the complete scripted control and the filtered
configured-model result. No configured credential file was read or copied.

These controls authorize a new frozen review within existing limits. They do
not establish provider response correctness or four-attempt pilot success.
Sampled RSS can miss brief peaks and double-count shared pages.
