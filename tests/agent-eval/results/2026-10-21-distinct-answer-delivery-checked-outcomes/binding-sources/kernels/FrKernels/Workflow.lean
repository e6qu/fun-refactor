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
  | checkOriginal
  | checkAcceptance
  deriving BEq, DecidableEq, Repr

-- fr:spec src/workflow.rs::workflow_stage_state @ 2f1c826f65afabbc2250cfe9aed44c64751a78a909594866c40d68e246bd7a3b
-- fr:signature applied: bool => applied: Bool; stage: usize => stage: Nat; return: usize => return: Nat
def stageState (applied : Bool) (stage : Nat) : Nat :=
  match applied, stage with
  | false, 0 => 1
  | false, 3 => 0
  | false, 4 => 1
  | false, 6 => 0
  | true, 1 => 1
  | true, 2 => 0
  | true, 5 => 1
  | true, 7 => 1
  | _, _ => 2

def step : State → Stage → Option State
  | .planned, .apply => some .applied
  | .planned, .checkRestored => some .planned
  | .planned, .redo => some .applied
  | .planned, .checkOriginal => some .planned
  | .applied, .checkApplied => some .applied
  | .applied, .checkAcceptance => some .applied
  | .applied, .undo => some .planned
  | .applied, .deliverPatch => some .applied
  | _, _ => none

def run : State → List Stage → Option State
  | state, [] => some state
  | state, stage :: rest => (step state stage).bind (fun next => run next rest)

def lifecycle (checkOriginal exerciseReversal deliverPatch : Bool) : List Stage :=
  (if checkOriginal then [.checkOriginal] else []) ++ [.apply, .checkApplied] ++
    (if exerciseReversal then [.undo, .checkRestored, .redo, .checkApplied] else []) ++
    (if deliverPatch then [.deliverPatch] else [])

def acceptedLifecycle (checkOriginal exerciseReversal deliverPatch : Bool) : List Stage :=
  (if checkOriginal then [.checkOriginal] else []) ++ [.apply, .checkApplied, .checkAcceptance] ++
    (if exerciseReversal then [.undo, .checkRestored, .redo, .checkApplied, .checkAcceptance] else []) ++
    (if deliverPatch then [.deliverPatch] else [])

theorem acceptance_requires_changed_state : step .planned .checkAcceptance = none := by
  rfl

theorem accepted_lifecycle_finishes_applied (original reversal patch : Bool) :
    run .planned (acceptedLifecycle original reversal patch) = some .applied := by
  cases original <;> cases reversal <;> cases patch <;> decide

theorem accepted_delivery_checks_outcome (original reversal : Bool) :
    (acceptedLifecycle original reversal true).getLast? = some .deliverPatch ∧
    (acceptedLifecycle original reversal true).contains .checkAcceptance = true := by
  cases original <;> cases reversal <;> decide

theorem generated_lifecycle_finishes_applied (checkOriginal exerciseReversal deliverPatch : Bool) :
    run .planned (lifecycle checkOriginal exerciseReversal deliverPatch) = some .applied := by
  cases checkOriginal <;> cases exerciseReversal <;> cases deliverPatch <;> decide

theorem delivery_requires_applied : step .planned .deliverPatch = none := by
  rfl

theorem restored_check_requires_planned : step .applied .checkRestored = none := by
  rfl

theorem original_check_requires_planned : step .applied .checkOriginal = none := by
  rfl

theorem reversal_round_trip :
    run .applied [.undo, .checkRestored, .redo, .checkApplied] = some .applied := by
  rfl

end FrKernels.Workflow
