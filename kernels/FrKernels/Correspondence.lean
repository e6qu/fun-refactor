namespace FrKernels.Correspondence

inductive Status where
  | missing | matched | ambiguous
  deriving DecidableEq, Repr

def classify (count : Nat) (shared : Bool) : Status :=
  match count, shared with
  | 0, _ => .missing
  | 1, false => .matched
  | _, _ => .ambiguous

theorem missing_has_no_candidates (shared : Bool) : classify 0 shared = .missing := by
  cases shared <;> rfl

theorem shared_candidate_is_ambiguous : classify 1 true = .ambiguous := by
  rfl

theorem multiple_candidates_are_ambiguous (n : Nat) (shared : Bool) :
    classify (n + 2) shared = .ambiguous := by
  cases shared <;> rfl

theorem matched_requires_unique_unshared (count : Nat) (shared : Bool)
    (h : classify count shared = .matched) : count = 1 ∧ shared = false := by
  cases count with
  | zero => simp [classify] at h
  | succ n =>
    cases n with
    | zero => cases shared <;> simp_all [classify]
    | succ n => simp [classify] at h

end FrKernels.Correspondence
