namespace FrKernels.Workflow

inductive State where
  | planned
  | applied
  deriving BEq, DecidableEq, Repr

inductive Stage where
  | apply
  | checkApplied
  | undo
  | checkRestored
  | redo
  | deliverPatch
  deriving BEq, DecidableEq, Repr

-- fr:spec src/workflow.rs::workflow_stage_state @ 94f5dc02b5a5234b0bcf88c720342b09377cf2046c9873a9b493b66e19e07246
-- fr:signature applied: bool => applied: Bool; stage: usize => stage: Nat; return: usize => return: Nat
def stageState (applied : Bool) (stage : Nat) : Nat :=
  match applied, stage with
  | false, 0 => 1
  | false, 3 => 0
  | false, 4 => 1
  | true, 1 => 1
  | true, 2 => 0
  | true, 5 => 1
  | _, _ => 2

def step : State → Stage → Option State
  | .planned, .apply => some .applied
  | .planned, .checkRestored => some .planned
  | .planned, .redo => some .applied
  | .applied, .checkApplied => some .applied
  | .applied, .undo => some .planned
  | .applied, .deliverPatch => some .applied
  | _, _ => none

def run : State → List Stage → Option State
  | state, [] => some state
  | state, stage :: rest => (step state stage).bind (fun next => run next rest)

def lifecycle (exerciseReversal deliverPatch : Bool) : List Stage :=
  [.apply, .checkApplied] ++
    (if exerciseReversal then [.undo, .checkRestored, .redo, .checkApplied] else []) ++
    (if deliverPatch then [.deliverPatch] else [])

theorem generated_lifecycle_finishes_applied (exerciseReversal deliverPatch : Bool) :
    run .planned (lifecycle exerciseReversal deliverPatch) = some .applied := by
  cases exerciseReversal <;> cases deliverPatch <;> decide

theorem delivery_requires_applied : step .planned .deliverPatch = none := by
  rfl

theorem restored_check_requires_planned : step .applied .checkRestored = none := by
  rfl

theorem reversal_round_trip :
    run .applied [.undo, .checkRestored, .redo, .checkApplied] = some .applied := by
  rfl

end FrKernels.Workflow
