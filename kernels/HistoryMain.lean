import FrKernels.History
import FrKernels.Patch

open FrKernels.History

def samples : List Snapshot :=
  [none, some ⟨"", 384⟩, some ⟨"λ\n", 384⟩, some ⟨"λ\n", 489⟩, some ⟨"名", 420⟩]

def patchModes : List UInt32 :=
  (List.range 4096).map UInt32.ofNat ++
    (List.range 32).map (fun bit => (1 : UInt32) <<< UInt32.ofNat bit) ++ [4294967295]

def patchModeCases : IO Unit := do
  for mode in patchModes do
    IO.println (FrKernels.Patch.gitMode mode).toNat
    for mask in ([0, 1, 8, 9, 64, 65, 72, 73, 146, 4294967295] : List UInt32) do
      IO.println (FrKernels.Patch.gitModeChangeSupported mode (mode ^^^ mask))

def patchSamples : List FrKernels.Patch.Snapshot :=
  none :: ["", "λ\n", "名", "a\x00b"].flatMap (fun content =>
    ([0, 1, 64, 73, 384, 420, 448, 493, 4095, 4294967295] : List UInt32).map
      (fun mode => some ⟨content, mode⟩))

def patchBasisCases : IO Unit := do
  for actual in patchSamples do
    for expected in patchSamples do
      IO.println (FrKernels.Patch.matchesPatchBasis actual expected)

def historyCases : IO Unit :=
  for current in samples do
    for before in samples do
      for after in samples do
        for recovery in [false, true] do
          IO.println (matchesSnapshot current before after recovery)

def main (args : List String) : IO Unit :=
  match args with
  | [] => historyCases
  | ["patch-modes"] => patchModeCases
  | ["patch-basis"] => patchBasisCases
  | _ => throw (IO.userError "expected patch-modes, patch-basis or no arguments")
