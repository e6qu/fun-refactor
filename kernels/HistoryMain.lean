import FrKernels.History
import FrKernels.MemoryHistory
import FrKernels.Patch
import FrKernels.Workflow
import FrKernels.TaskChange

open FrKernels.History

def samples : List Snapshot :=
  [none, some ⟨"", 384, false⟩, some ⟨"λ\n", 384, false⟩,
    some ⟨"λ\n", 489, false⟩, some ⟨"名", 420, false⟩, some ⟨"target", 0, true⟩]

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
      (fun mode => some ⟨content, mode, false⟩)) ++
    [some ⟨"target", 0, true⟩, some ⟨"other", 0, true⟩]

def patchBasisCases : IO Unit := do
  for actual in patchSamples do
    for expected in patchSamples do
      IO.println (FrKernels.Patch.matchesPatchBasis actual expected)

def ownerExecutableCases : IO Unit := do
  for mode in patchModes do
    for executable in [false, true] do
      IO.println (FrKernels.Patch.ownerExecutableMode mode executable).toNat

def snapshotModeCases : IO Unit := do
  for mode in patchModes do
    for symlink in [false, true] do
      IO.println (FrKernels.Patch.gitSnapshotMode symlink mode).toNat

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
  | ["owner-executable"] => ownerExecutableCases
  | ["snapshot-modes"] => snapshotModeCases
  | ["workflow-stages"] =>
      for applied in [false, true] do
        for stage in List.range 6 do
          IO.println (FrKernels.Workflow.stageState applied stage)
  | ["task-change-modes"] =>
      for completeReview in [false, true] do
        for write in [false, true] do
          for basisSupplied in [false, true] do
            for basisMatches in [false, true] do
              IO.println (FrKernels.TaskChange.mode completeReview write basisSupplied basisMatches)
  | ["memory-transitions"] =>
      for status in List.range 4 do
        for action in List.range 3 do
          for atTop in [false, true] do
            IO.println (FrKernels.MemoryHistory.transitionAllowed status action atTop)
  | _ => throw (IO.userError "expected patch-modes, patch-basis, owner-executable, snapshot-modes, workflow-stages, task-change-modes, memory-transitions or no arguments")
