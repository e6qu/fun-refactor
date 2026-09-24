import FrKernels.Correspondence
open FrKernels.Correspondence

def main : IO Unit := do
  for count in List.range 8 do
    for shared in [false, true] do
      IO.println (match classify count shared with
        | .missing => "missing"
        | .matched => "matched"
        | .ambiguous => "ambiguous")
