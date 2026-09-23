# Investigations

Call, reference and flow reports expose revision-bound occurrence/origin records. Read exact byte
spans and Unicode line ranges separately from enclosing declaration handles. Missing or multiple
origins remain explicit.

`fr project dataflow HANDLE` follows the declared Python scalar subset through same-file helpers.
Use `--steps`, `--depth` and `--bytes` to bound work and output. External source, sink, propagation
and sanitizer contracts come from `--rules`; `--context` selects the sanitizer context. Check every
cutoff and omission. A report describes explicit value propagation with unchecked path feasibility.
It cannot establish application security. Loops, recursion, aliases and unsupported effects remain
incomplete boundaries.

`fr project investigate --from PLAN` revalidates a local plan and returns updated states without
writing or executing actions. Use `--transition STEP:start`, `STEP:satisfy`, `STEP:block` or
`STEP:reset` for explicit transitions. Preserve hypotheses, open questions and required checks.
Evidence binds input digests and task criteria. Changed inputs invalidate dependent work. Use a
workspace dependency unless narrower dependency coverage is complete. References record reported
checks and proof kinds; they do not attest execution or authorize mutation.

The Python `fr_ir.investigation.TaskPlan` provides typed plans and persistence through the existing
verified Merkle store. Keep that store outside the analyzed workspace. Resume after restoration
before relying on retained evidence.

`fr project identities` pages declaration content digests beside current handles. Pass a retained
report with `--from` to inspect matched, ambiguous and missing correspondence. These are syntactic
candidates; they never renew a stale mutation review. Follow [Task](task.md) and [Workflow](workflow.md)
for checked changes, reversal and patch delivery.
