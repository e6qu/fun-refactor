# Explain a finding and keep it useful

For “does this input reach send?”, connect the reported calls to their source lines.
A source supplies the configured input; a sink consumes it. A witness is a possible connection in the supported model.
It is not a recorded execution. A cutoff explains why analysis could not finish.

## Ask the value question

Run `fr project dataflow '<HANDLE>' --summaries --rules rules.json --context html`.

Obtain the handle from a project query. The rules file declares sources, sinks, propagation and context-specific sanitizers.
These are assumptions about external calls, not behaviors the tool discovers automatically.
`--steps`, `--depth` and `--bytes` bound work and output.
Read completion, cutoffs and omissions with the witnesses. An incomplete empty report cannot rule out a connection.

A function summary describes which argument origins can contribute to its result or supported actions.
`--summaries` supports selected recursive same-file functions. The default route cuts off recursion.
Repeated analysis reaching a stable result does not prove the function terminates at runtime.
Exception handlers, shared mutable objects and unsupported calls can leave the answer incomplete.

## Check where the answer comes from

Call, reference and flow reports identify individual uses and source locations in one revision.
A byte range selects written text; the enclosing handle selects its declaration.
Some generated nodes have no exact source origin. Keep missing or ambiguous mappings visible.

Run `fr project semantic '<HANDLE>' --body --origins --origin-limit 40` for source mappings.
Use `fr project flow-facts '<HANDLE>' --rules rules.json --context html --limit 2` for paged explanations.

Follow returned `origins.continuation` and fact explanation actions without dropping their input options.
A complete explanation covers one fact; it does not make an incomplete overall analysis complete.
`SemanticOrigins` connects source locations to structured edit pointers.
`FlowFacts` checks and stores report pages without renewing stale actions.

## Resume without trusting outdated findings

A task plan stores questions, hypotheses, observations, dependencies and required checks.
`fr project investigate --from PLAN` rechecks a local plan without writing or executing actions.
Explicit transitions use `--transition STEP:start`, `:satisfy`, `:block` or `:reset`.
Changed inputs invalidate dependent findings; unrelated notes can survive.

Python `TaskPlan` stores plans in a verified object store outside the analyzed project.
Resume restored plans before using them. `TaskStep.checked` names check inputs; inspect `checks --toolchain` before execution.
Candidate matches from `project identities --from REPORT` never renew an edit review.

`FlowCache` stores complete results and validates inputs before reuse. Keep its trusted root fingerprint.
Stale inputs recompute; altered records refuse. Reuse renews finding locations, not permission to write.
Measure validation and storage overhead against fresh analysis before claiming a benefit.
Use [task changes](task.md) and [workflows](workflow.md) for checked delivery and reversal.
