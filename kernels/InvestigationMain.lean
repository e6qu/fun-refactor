import FrKernels.Investigation
open FrKernels.Investigation

def main : IO Unit := do
  let states := [State.pending, .ready, .running, .satisfied, .blocked, .stale]
  for initial in states do
    for target in states do
      for prerequisites in [false, true] do
        for evidence in [false, true] do
          IO.println (transitionAllowed initial target prerequisites evidence)
