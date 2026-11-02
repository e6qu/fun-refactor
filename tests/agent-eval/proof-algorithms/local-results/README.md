# Checked records, with their limits

The [original Kimi patch recheck](interrupted-kimi-recheck.json) accepts the exact
submitted proof from the interrupted batch. It proves both claims for the original
GCD function. It cannot repair that batch's incomplete resource accounting or show
that one kind of guidance helped.

The [earlier initial-goal checks](native-admission.json) use scripted replies over the
local loopback connection. Luna, GLM and Kimi make no model calls. Their actual CLI,
fr and Lean inspect the revised task together within the existing resource limits.
Two failed Codex setup checks remain listed too.
These checks inspect only the initial goal. They do not satisfy the current
[complete-workflow requirement](../README.md#what-must-pass-before-we-call-an-agent).
That requirement also covers rejected drafts, proof saves, the package, patch and accounting.

These are tool checks. Fresh agent comparisons and independent patch grading are
still required. Raw conversations, commands and source snapshots remain private.
