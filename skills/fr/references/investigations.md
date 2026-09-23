# Investigations

Call, reference and flow reports expose revision-bound occurrence/origin records. Read exact byte
spans and Unicode line ranges separately from enclosing declaration handles. Missing or multiple
origins remain explicit.

`fr project dataflow HANDLE` follows the declared Python scalar subset through same-file helpers.
Use `--steps`, `--depth` and `--bytes` to bound work and output. External source, sink, propagation
and sanitizer contracts come from `--rules`; `--context` selects the sanitizer context. Check every
cutoff and omission. A report describes explicit value propagation with unchecked path feasibility.
It cannot establish application security. While loops use bounded fixed-point analysis. Recursion,
aliases, exception handlers and unsupported effects remain incomplete boundaries.

Read `control_flow`, `origins` and `summaries` for exact graph nodes and context-specific evaluations.
Synthetic exits have absent origins. Edges use local IDs; derivations do not prove path feasibility.
`--inputs-only` fingerprints the defining file, rules, configuration, analyzer and budgets.
Python `fr_ir.flow.FlowCache` stores complete results in a verified Merkle store and revalidates
inputs before reuse. Keep its trusted root; stale inputs recompute and tampered objects refuse.
Reuse renews occurrence handles, never mutation reviews. Measure overhead before relying on speedups.

`fr project investigate --from PLAN` revalidates a local plan and returns updated states without
writing or executing actions. Use `--transition STEP:start`, `STEP:satisfy`, `STEP:block` or
`STEP:reset` for explicit transitions. Preserve hypotheses, open questions and required checks.
Evidence binds input digests and task criteria. Changed inputs invalidate dependent work. Use a
workspace dependency unless narrower dependency coverage is complete. References record reported
checks and proof kinds; they do not attest execution or authorize mutation.

The Python `fr_ir.investigation.TaskPlan` provides typed plans and persistence through the existing
verified Merkle store. Keep that store outside the analyzed workspace. Resume after restoration
before relying on retained evidence.

For Python body provenance, use `project semantic HANDLE --body --origins --origin-limit 40`.
Follow `origins.continuation` or select one `--origin-pointer`. Missing mappings remain explicit.
`SemanticOrigins` links those records to authoring pointers and same-revision flow occurrences.
Use `TaskStep.from_guide` to retain ready guide input. `TaskStep.checked` declares the input scope
for `investigation_checks.run_checks`; inspect `checks --toolchain` before executing its declarations.

`fr project identities` pages declaration content digests beside current handles. Pass a retained
report with `--from` to inspect matched, ambiguous and missing correspondence. These are syntactic
candidates; they never renew a stale mutation review. Follow [Task](task.md) and [Workflow](workflow.md)
for checked changes, reversal and patch delivery.
