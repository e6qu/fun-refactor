# Smaller catalog, unchanged resource limits

[Hosted run 37567717621](https://github.com/e6qu/fun-refactor/actions/runs/37567717621)
compared three default captures with three empty-catalog captures on each platform.
All six catalog captures completed the same two-response source-delivery control
below the 640 MiB admission target. Linux peaked at 465–475 MiB; macOS at 542–559
MiB. All three default macOS captures hit the unchanged 768 MiB stop limit.

One guarded workstation control then completed at 535 MiB sampled aggregate RSS,
2 CPU seconds and 4.8 seconds elapsed. Its OpenCode executable hash matched the
hosted macOS executable. A separate bounded, model-free `opencode --pure models`
check listed both configured model identifiers with this override, using 315 MiB.
The client resolved its configuration normally; no credential file was inspected.

The override is `OPENCODE_MODELS_PATH` pointing to a file containing exactly
`{}\n`. It applies only to each child process. Global configuration is unchanged.
This establishes room for a fresh local review, not live-provider compatibility
or a guarantee about longer code-change attempts. The first resource-limit
failure must stop the new collection. No earlier stopped collection may resume.

The archives retain complete hosted measurements and the workstation scripted
control. `manifest.json` binds each archive and the frozen measurement scripts.
`report.json` contains their independently replayed results. The model listing
retains only expected identifier membership, output sizes and process counters.
The local scripts are historical invocation records, not reusable commands.

Sampled RSS can miss brief peaks and double-count shared pages. The experiment
identifies an effective setting; it does not attribute particular heap objects.
