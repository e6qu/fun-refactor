namespace FrKernels.Digest

structure State (α : Type) where
  emitted : List α := []
  pending : List α := []
  deriving BEq, Repr

def bytes (state : State α) : List α := state.emitted ++ state.pending

def flush (state : State α) : State α := ⟨bytes state, []⟩

def commit (threshold : Nat) (state : State α) : State α :=
  if threshold ≤ state.pending.length then flush state else state

inductive Operation (α : Type) where
  | success (fragment : List α)
  | failure (incomplete : List α)
  | flush
  deriving Repr

def step (threshold : Nat) (state : State α) : Operation α → State α
  | .success fragment => commit threshold ⟨state.emitted, state.pending ++ fragment⟩
  | .failure incomplete => ⟨state.emitted, (state.pending ++ incomplete).take state.pending.length⟩
  | .flush => flush state

def accepted : List (Operation α) → List α
  | [] => []
  | .success fragment :: rest => fragment ++ accepted rest
  | .failure _ :: rest => accepted rest
  | .flush :: rest => accepted rest

def run (threshold : Nat) (state : State α) : List (Operation α) → State α
  | [] => state
  | op :: rest => run threshold (step threshold state op) rest

def finish (state : State α) : List α := (flush state).emitted

theorem flush_preserves_bytes (state : State α) : bytes (flush state) = bytes state := by
  simp [bytes, flush]

theorem flush_empties_pending (state : State α) : (flush state).pending = [] := rfl

theorem flush_is_idempotent (state : State α) : flush (flush state) = flush state := by
  simp [flush, bytes]

theorem finish_preserves_bytes (state : State α) : finish state = bytes state := rfl

theorem finish_after_flush (state : State α) : finish (flush state) = finish state := by
  exact flush_preserves_bytes state

theorem commit_preserves_bytes (threshold : Nat) (state : State α) :
    bytes (commit threshold state) = bytes state := by
  unfold commit
  split <;> simp [flush_preserves_bytes]

theorem commit_flushes_at_threshold (threshold : Nat) (state : State α)
    (full : threshold ≤ state.pending.length) : commit threshold state = flush state := by
  simp [commit, full]

theorem commit_retains_below_threshold (threshold : Nat) (state : State α)
    (small : state.pending.length < threshold) : commit threshold state = state := by
  simp [commit, Nat.not_le.mpr small]

theorem commit_pending_bounded (threshold : Nat) (state : State α) (positive : 0 < threshold) :
    (commit threshold state).pending.length < threshold := by
  unfold commit
  split
  · simpa [flush] using positive
  · omega

theorem failed_append_restores_state (threshold : Nat) (state : State α) (incomplete : List α) :
    step threshold state (.failure incomplete) = state := by
  cases state
  simp [step]

theorem successful_append_preserves_order (threshold : Nat) (state : State α) (fragment : List α) :
    bytes (step threshold state (.success fragment)) = bytes state ++ fragment := by
  simp only [step, commit_preserves_bytes]
  simp [bytes, List.append_assoc]

theorem step_preserves_pending_bound (threshold : Nat) (state : State α) (op : Operation α)
    (positive : 0 < threshold) (bounded : state.pending.length < threshold) :
    (step threshold state op).pending.length < threshold := by
  cases op with
  | success fragment => exact commit_pending_bounded threshold _ positive
  | failure incomplete => simpa [failed_append_restores_state] using bounded
  | flush => simpa [step, flush] using positive

theorem run_preserves_order (threshold : Nat) (state : State α) (ops : List (Operation α)) :
    bytes (run threshold state ops) = bytes state ++ accepted ops := by
  induction ops generalizing state with
  | nil => simp [run, accepted]
  | cons op rest ih =>
    simp only [run, ih]
    cases op with
    | success fragment => simp [accepted, successful_append_preserves_order, List.append_assoc]
    | failure incomplete => simp [accepted, failed_append_restores_state]
    | flush => simp [accepted, step, flush_preserves_bytes]

theorem run_preserves_pending_bound (threshold : Nat) (state : State α) (ops : List (Operation α))
    (positive : 0 < threshold) (bounded : state.pending.length < threshold) :
    (run threshold state ops).pending.length < threshold := by
  induction ops generalizing state with
  | nil => exact bounded
  | cons op rest ih => exact ih _ (step_preserves_pending_bound threshold state op positive bounded)

theorem run_composes (threshold : Nat) (state : State α) (first second : List (Operation α)) :
    run threshold state (first ++ second) = run threshold (run threshold state first) second := by
  induction first generalizing state with
  | nil => rfl
  | cons op rest ih => exact ih (step threshold state op)

theorem finalization_preserves_all_successful_bytes (threshold : Nat) (state : State α)
    (ops : List (Operation α)) : finish (run threshold state ops) = bytes state ++ accepted ops := by
  exact run_preserves_order threshold state ops

theorem thresholds_preserve_final_bytes (first second : Nat) (state : State α) (ops : List (Operation α)) :
    finish (run first state ops) = finish (run second state ops) := by
  simp [finalization_preserves_all_successful_bytes]

theorem flush_schedule_preserves_final_bytes (threshold : Nat) (state : State α)
    (first second : List (Operation α)) :
    finish (run threshold state (first ++ .flush :: second)) = finish (run threshold state (first ++ second)) := by
  simp only [run_composes, run, finish_preserves_bytes, run_preserves_order, step, flush_preserves_bytes]

theorem failed_write_preserves_future_bytes (threshold : Nat) (state : State α)
    (incomplete : List α) (rest : List (Operation α)) :
    run threshold state (.failure incomplete :: rest) = run threshold state rest := by
  simp [run, failed_append_restores_state]

def digestView (update : σ → List α → σ) (initial : σ) (state : State α) : σ :=
  update (update initial state.emitted) state.pending

theorem digest_view_equals_ordered_bytes (update : σ → List α → σ) (initial : σ) (state : State α)
    (chunkLaw : ∀ seed left right, update seed (left ++ right) = update (update seed left) right) :
    digestView update initial state = update initial (bytes state) := by
  exact (chunkLaw initial state.emitted state.pending).symm

theorem digest_view_preserves_thresholds (update : σ → List α → σ) (initial : σ)
    (first second : Nat) (state : State α) (ops : List (Operation α))
    (chunkLaw : ∀ seed left right, update seed (left ++ right) = update (update seed left) right) :
    digestView update initial (run first state ops) = digestView update initial (run second state ops) := by
  simp [digest_view_equals_ordered_bytes update initial _ chunkLaw, run_preserves_order]

end FrKernels.Digest
