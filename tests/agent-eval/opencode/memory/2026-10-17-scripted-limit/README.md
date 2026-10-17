# A scripted review stopped at the memory limit

The [failed CI job](https://github.com/e6qu/fun-refactor/actions/runs/39031180016/job/117391774611)
tested a local fake provider, with no paid model calls.
The first intentionally unsuccessful review exceeded its 768 MiB process-group limit after one completed response.
Its sampled peak was 805,642,240 bytes, above the 805,306,368-byte limit.
The guard stopped the processes. The two-step assertion then failed because the second response had not finished.
The next review failed as scripted, and the third stayed unstarted under the existing stop rule.

`ci-artifact.zip` preserves the original job artifact, including failed captures, provider requests and partial work records.
`manifest.json` records its digest, original revision and GitHub identifiers.
The offline regression test compares the saved work count with the retained completion events.
It keeps the resource stop visible as a failed attempt.

A model catalog lists providers, models and their settings.
Disabling catalog downloads still leaves OpenCode's bundled catalog active in the
[pinned loader](https://github.com/anomalyco/opencode/blob/v1.18.34/packages/core/src/models-dev.ts#L201).
The scripted job did not select the smaller catalog. Its isolated environment also omitted the catalog setting when supplied.
The corrected CI job supplies the existing [small verified catalog](../../catalog/README.md) and preserves its path when launching the client.
It checks that catalog before starting the client. The fake provider's model remains explicitly configured by each control.

The 768 MiB memory limit, per-attempt deadlines and five-minute job deadline stay unchanged.
The two-response, source-delivery, partial-work and stop-rule assertions remain required.
Earlier [catalog measurements](../2026-10-07-providers/README.md) describe their own six captures and endpoint checks.
This failure records one sampled memory stop. It does not measure model quality, complete agent tasks or savings from fr.
