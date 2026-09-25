namespace FrKernels.HostRecovery

-- fr:spec src/transaction_kernel.rs::history_publication_allowed @ 178658465d54cbb912ef01d777a0c219608d9635352f42424e422dee8ee497c8
-- fr:signature target_matches: bool => targetMatches: Bool; staged_matches: bool => stagedMatches: Bool; return: bool => return: Bool
def publicationAllowed (targetMatches stagedMatches : Bool) : Bool :=
  targetMatches && stagedMatches

theorem publication_requires_both (target staged : Bool)
    (accepted : publicationAllowed target staged = true) :
    target = true ∧ staged = true := by
  simpa [publicationAllowed] using accepted

theorem altered_stage_refuses (target : Bool) :
    publicationAllowed target false = false := by simp [publicationAllowed]

-- fr:spec src/transaction_kernel.rs::history_recovery_step @ ee77bebd335a3ed9955e91be1b738919db8e28a60947819b564761b05b44cc41
-- fr:signature matches_before: bool => matchesBefore: Bool; matches_after: bool => matchesAfter: Bool; return: usize => return: Nat
def recoveryStep (matchesBefore matchesAfter : Bool) : Nat :=
  if matchesBefore then 0 else if matchesAfter then 1 else 2

theorem restored_paths_need_no_write (after : Bool) :
    recoveryStep true after = 0 := by simp [recoveryStep]

theorem unrelated_state_refuses : recoveryStep false false = 2 := by decide

theorem restore_requires_written_snapshot (before after : Bool)
    (restore : recoveryStep before after = 1) :
    before = false ∧ after = true := by
  cases before <;> cases after <;> simp_all [recoveryStep]

def restore {α : Type} [DecidableEq α] (current before after : α) : Option α :=
  if current = before ∨ current = after then some before else none

theorem restore_success_is_starting_state {α : Type} [DecidableEq α]
    (current before after result : α) (accepted : restore current before after = some result) :
    result = before := by
  unfold restore at accepted
  split at accepted <;> simp_all

theorem restore_idempotent {α : Type} [DecidableEq α] (current before after : α) :
    (restore current before after).bind (fun restored => restore restored before after) =
      restore current before after := by
  unfold restore
  split <;> simp_all

theorem restore_conflict_refuses {α : Type} [DecidableEq α] (current before after : α)
    (notBefore : current ≠ before) (notAfter : current ≠ after) :
    restore current before after = none := by
  simp [restore, notBefore, notAfter]

end FrKernels.HostRecovery
