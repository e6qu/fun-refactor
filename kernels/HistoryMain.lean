import FrKernels.History
import FrKernels.MemoryHistory
import FrKernels.Patch
import FrKernels.Workflow
import FrKernels.TaskChange
import FrKernels.AgentSession
import FrKernels.AgentContext

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
        for stage in List.range 7 do
          IO.println (FrKernels.Workflow.stageState applied stage)
  | ["task-change-modes"] =>
      for completeReview in [false, true] do
        for write in [false, true] do
          for basisSupplied in [false, true] do
            for basisMatches in [false, true] do
              IO.println (FrKernels.TaskChange.mode completeReview write basisSupplied basisMatches)
  | ["agent-session-steps"] =>
      for state in List.range 3 do
        for action in List.range 2 do
          for previewValid in [false, true] do
            for manifestMatches in [false, true] do
              for basisMatches in [false, true] do
                IO.println (FrKernels.AgentSession.step state action previewValid manifestMatches basisMatches)
  | ["agent-context-admission"] => do
      for calls in ([0, 1, 63, 64, 65, 18446744073709551615] : List Nat) do
        for limit in ([0, 1, 63, 64, 65, 18446744073709551615] : List Nat) do
          for sessionMatches in [false, true] do
            for complete in [false, true] do
              for digestMatches in [false, true] do
                IO.println (FrKernels.AgentContext.materializationAdmitted
                  calls limit sessionMatches complete digestMatches)
      for objects in ([0, 1, 65535, 65536, 65537, 18446744073709551615] : List Nat) do
        for bytes in ([0, 1, 67108863, 67108864, 67108865, 18446744073709551615] : List Nat) do
          for digestMatches in [false, true] do
            for recordsCanonical in [false, true] do
              for rootPresent in [false, true] do
                IO.println (FrKernels.AgentContext.objectStoreAdmitted
                  objects bytes digestMatches recordsCanonical rootPresent)
  | ["memory-transitions"] =>
      for status in List.range 4 do
        for action in List.range 3 do
          for atTop in [false, true] do
            IO.println (FrKernels.MemoryHistory.transitionAllowed status action atTop)
  | ["memory-restores"] =>
      for schemaMatches in [false, true] do
        for digestMatches in [false, true] do
          for historyValid in [false, true] do
            for files in [0, 4096, 4097, 18446744073709551615] do
              for payloadBytes in [0, 4194304, 4194305, 18446744073709551615] do
                IO.println (FrKernels.MemoryHistory.restoreAllowed
                  schemaMatches digestMatches historyValid files payloadBytes)
  | ["memory-compactions"] =>
      for keep in [0, 1, 255, 256, 257, 18446744073709551615] do
        IO.println (FrKernels.MemoryHistory.compactionAllowed keep)
  | ["record-compaction"] =>
      for detailed in [false, true] do
        for pending in [false, true] do
          for retained in [false, true] do
            for planned in [false, true] do
              IO.println (recordCompactable detailed pending retained planned)
  | ["file-move"] =>
      for sourceExists in [false, true] do
        for destinationExists in [false, true] do
          for distinctPaths in [false, true] do
            for sourceSupported in [false, true] do
              IO.println (moveAdmitted sourceExists destinationExists distinctPaths sourceSupported)
  | _ => throw (IO.userError "expected patch-modes, patch-basis, owner-executable, snapshot-modes, workflow-stages, task-change-modes, agent-session-steps, agent-context-admission, memory-transitions, memory-restores, memory-compactions, record-compaction, file-move or no arguments")
