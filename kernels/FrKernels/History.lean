namespace FrKernels.History

structure FileSnapshot where
  content : String
  mode : Nat
  deriving DecidableEq, Repr

abbrev Snapshot := Option FileSnapshot

-- fr:spec src/history.rs::matches_snapshot @ 6ea40f4e
-- fr:signature current: &Option<Snapshot> => current: Snapshot; before: &Option<Snapshot> => before: Snapshot; after: &Option<Snapshot> => after: Snapshot; recovery: bool => recovery: Bool; return: bool => return: Bool
def matchesSnapshot (current : Snapshot) (before : Snapshot) (after : Snapshot) (recovery : Bool) : Bool :=
  decide (current = before) || (recovery && decide (current = after))

def replaceChecked (current before after : Snapshot) : Option Snapshot :=
  if matchesSnapshot current before after false then some after else none

theorem apply_accepts_its_basis (before after : Snapshot) :
    replaceChecked before before after = some after := by
  simp [replaceChecked, matchesSnapshot]

theorem undo_after_apply_restores_snapshot (before after : Snapshot) :
    (replaceChecked before before after).bind (fun current => replaceChecked current after before) = some before := by
  simp [apply_accepts_its_basis]

theorem conflicting_transition_refuses (current before after : Snapshot) (different : current ≠ before) :
    replaceChecked current before after = none := by
  simp [replaceChecked, matchesSnapshot, different]

theorem recovery_accepts_before (before after : Snapshot) :
    matchesSnapshot before before after true = true := by
  simp [matchesSnapshot]

theorem recovery_accepts_after (before after : Snapshot) :
    matchesSnapshot after before after true = true := by
  simp [matchesSnapshot]

theorem recovery_rejects_other (current before after : Snapshot)
    (notBefore : current ≠ before) (notAfter : current ≠ after) :
    matchesSnapshot current before after true = false := by
  simp [matchesSnapshot, notBefore, notAfter]

def recoverChecked : List Snapshot → List Snapshot → List Snapshot → Option (List Snapshot)
  | [], [], [] => some []
  | before :: bs, after :: as, current :: cs =>
      if matchesSnapshot current before after true then
        (recoverChecked bs as cs).map (before :: ·)
      else none
  | _, _, _ => none

inductive Compatible : List Snapshot → List Snapshot → List Snapshot → Prop where
  | nil : Compatible [] [] []
  | cons {b a c : Snapshot} {bs as cs : List Snapshot} :
      matchesSnapshot c b a true = true → Compatible bs as cs → Compatible (b :: bs) (a :: as) (c :: cs)

theorem recovery_restores_mixed_snapshots (before after current : List Snapshot)
    (compatible : Compatible before after current) :
    recoverChecked before after current = some before := by
  induction compatible with
  | nil => rfl
  | cons accepted _ ih => simp [recoverChecked, accepted, ih]

structure Stacks where
  applied : List Nat
  redo : List Nat
  deriving BEq, DecidableEq

def applyNew (id : Nat) (state : Stacks) : Stacks :=
  ⟨id :: state.applied, []⟩

def undo (state : Stacks) : Option Stacks :=
  match state.applied with
  | [] => none
  | id :: rest => some ⟨rest, id :: state.redo⟩

def redo (state : Stacks) : Option Stacks :=
  match state.redo with
  | [] => none
  | id :: rest => some ⟨id :: state.applied, rest⟩

theorem redo_after_undo (id : Nat) (applied redos : List Nat) :
    (undo ⟨id :: applied, redos⟩).bind redo = some ⟨id :: applied, redos⟩ := by
  rfl

theorem undo_after_redo (id : Nat) (applied redos : List Nat) :
    (redo ⟨applied, id :: redos⟩).bind undo = some ⟨applied, id :: redos⟩ := by
  rfl

theorem new_apply_clears_redo (id : Nat) (state : Stacks) :
    (applyNew id state).redo = [] := by
  rfl

end FrKernels.History
