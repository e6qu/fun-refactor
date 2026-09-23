import FrKernels.Investigation
import FrKernels.Flow
open FrKernels.Investigation

def main (args : List String) : IO Unit := do
  if args == ["flow-join"] then
    for left in List.range 16 do
      for right in List.range 16 do
        for boundLeft in [false, true] do
          for boundRight in [false, true] do
            let joined := FrKernels.Flow.joinMask left right
            let bound := boundLeft && boundRight
            IO.println s!"{joined} {bound} {joined != left || bound != boundLeft}"
    return
  let states := [State.pending, .ready, .running, .satisfied, .blocked, .stale]
  for initial in states do
    for target in states do
      for prerequisites in [false, true] do
        for evidence in [false, true] do
          IO.println (transitionAllowed initial target prerequisites evidence)
