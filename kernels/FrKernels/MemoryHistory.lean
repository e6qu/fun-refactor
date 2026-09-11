namespace FrKernels.MemoryHistory

-- fr:spec src/transaction_kernel.rs::memory_transition_allowed @ e23a2e128da5820dc1f726ff68ee74aa9a85ee8d932f22e956fdfc6fe5a8347a
-- fr:signature status: usize => status: Nat; action: usize => action: Nat; at_stack_top: bool => atStackTop: Bool; return: bool => return: Bool
def transitionAllowed (status : Nat) (action : Nat) (atStackTop : Bool) : Bool :=
  atStackTop && ((status == 0 && action == 0) || (status == 1 && action == 1))

theorem abandoned_never_transitions (action : Nat) (atTop : Bool) :
    transitionAllowed 2 action atTop = false := by
  simp [transitionAllowed]

theorem non_top_never_transitions (status action : Nat) :
    transitionAllowed status action false = false := by
  simp [transitionAllowed]

theorem applied_only_undoes_at_top :
    transitionAllowed 0 0 true = true ∧ transitionAllowed 0 1 true = false := by
  decide

theorem undone_only_redoes_at_top :
    transitionAllowed 1 1 true = true ∧ transitionAllowed 1 0 true = false := by
  decide

def replaceManyChecked {α : Type} [DecidableEq α]
    (current before after : List α) : Option (List α) :=
  if current = before then some after else none

theorem checked_many_apply_accepts_basis {α : Type} [DecidableEq α]
    (before after : List α) :
    replaceManyChecked before before after = some after := by
  simp [replaceManyChecked]

theorem checked_many_conflict_refuses_all {α : Type} [DecidableEq α]
    (current before after : List α) (different : current ≠ before) :
    replaceManyChecked current before after = none := by
  simp [replaceManyChecked, different]

theorem checked_many_undo_after_apply_restores_all {α : Type} [DecidableEq α]
    (before after : List α) :
    (replaceManyChecked before before after).bind
      (fun current => replaceManyChecked current after before) = some before := by
  simp [replaceManyChecked]

end FrKernels.MemoryHistory
