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

-- fr:spec src/transaction_kernel.rs::memory_restore_allowed @ 58a22f5a0985ca790a8f7cede2b233f2046adb670a1336e0a8bdb5fbe98e8610
-- fr:signature schema_matches: bool => schemaMatches: Bool; digest_matches: bool => digestMatches: Bool; history_valid: bool => historyValid: Bool; files: usize => files: Nat; payload_bytes: usize => payloadBytes: Nat; return: bool => return: Bool
def restoreAllowed
    (schemaMatches : Bool)
    (digestMatches : Bool)
    (historyValid : Bool)
    (files : Nat)
    (payloadBytes : Nat) : Bool :=
  schemaMatches && digestMatches && historyValid &&
    decide (files ≤ 4096 ∧ payloadBytes ≤ 4194304)

theorem restore_accepts_exact_bounded_session
    (files payloadBytes : Nat) (filesBound : files ≤ 4096)
    (payloadBound : payloadBytes ≤ 4194304) :
    restoreAllowed true true true files payloadBytes = true := by
  simp [restoreAllowed, filesBound, payloadBound]

theorem restore_rejects_wrong_schema
    (digest history : Bool) (files payloadBytes : Nat) :
    restoreAllowed false digest history files payloadBytes = false := by
  simp [restoreAllowed]

theorem restore_rejects_wrong_digest
    (history : Bool) (files payloadBytes : Nat) :
    restoreAllowed true false history files payloadBytes = false := by
  simp [restoreAllowed]

theorem restore_rejects_invalid_history (files payloadBytes : Nat) :
    restoreAllowed true true false files payloadBytes = false := by
  simp [restoreAllowed]

theorem restore_rejects_too_many_files (files payloadBytes : Nat)
    (tooMany : 4096 < files) :
    restoreAllowed true true true files payloadBytes = false := by
  simp [restoreAllowed, Nat.not_le_of_lt tooMany]

theorem restore_rejects_oversized_payload (files payloadBytes : Nat)
    (tooLarge : 4194304 < payloadBytes) :
    restoreAllowed true true true files payloadBytes = false := by
  simp [restoreAllowed, Nat.not_le_of_lt tooLarge]

-- fr:spec src/transaction_kernel.rs::memory_compaction_allowed @ a1f0b0b9a58050ed9ddf7e43274ce51cb7d0cc16cace20d2eab6a5cb516a7c87
-- fr:signature keep: usize => keep: Nat; return: bool => return: Bool
def compactionAllowed (keep : Nat) : Bool := decide (keep ≤ 256)

theorem compaction_accepts_boundary : compactionAllowed 256 = true := by decide

theorem compaction_rejects_above_boundary (keep : Nat) (tooMany : 256 < keep) :
    compactionAllowed keep = false := by
  simp [compactionAllowed, Nat.not_le_of_lt tooMany]

def retainNewest {α : Type} (keep : Nat) (entries : List α) : List α :=
  entries.drop (entries.length - keep)

theorem retain_none_is_empty {α : Type} (entries : List α) :
    retainNewest 0 entries = [] := by
  simp [retainNewest]

theorem retain_everything_at_length {α : Type} (entries : List α) :
    retainNewest entries.length entries = entries := by
  simp [retainNewest]

end FrKernels.MemoryHistory
