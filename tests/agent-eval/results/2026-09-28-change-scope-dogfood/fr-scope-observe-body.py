"""Attach discovery to an explicitly running step; never refresh stale prerequisites."""
self.__post_init__()
if not self.ready:
    raise FrRuntimeError("incomplete scope cannot satisfy a discovery step")
original = [s for s in plan.steps if s.id == step]
resumed = plan.resume(client)
selected = [s for s in resumed.plan.steps if s.id == step]
if (len(original) != 1 or original[0].state != StepState.RUNNING
        or len(selected) != 1 or selected[0].state != StepState.READY
        or self.dependency not in selected[0].inputs):
    raise FrRuntimeError("scope observation requires matching current inputs on a running step")
evidence = Evidence("change-scope", EvidenceKind.OBSERVATION, resumed.input_digests[step],
                    True, "fr-change-scope:" + (self.dependency.digest or ""))
steps = tuple(replace(s, state=StepState.RUNNING, evidence=tuple(e for e in s.evidence if e.id != evidence.id)+(evidence,))
              if s.id == step else s for s in resumed.plan.steps)
return replace(resumed.plan, steps=steps).resume(client, transition=f"{step}:satisfy")