import Std

theorem simplify_hypothesis (n : Nat) (h : n + 0 = 0) : n = 0 := by
  simp only [Nat.add_zero] at h
  exact h

theorem simplify_everywhere (n : Nat) (h : n + 0 = 0) : n + 0 = 0 := by
  simp only [Nat.add_zero] at h ⊢
  exact h

theorem transport_equality (a b : Nat) (h : a + b = 0) : b + a = 0 := by
  exact Nat.add_comm a b ▸ h

def guardedRange (offset count total : Nat) : Bool :=
  (offset <= total) && (count <= total - offset)

theorem nested_locations (offset count total : Nat) :
    (guardedRange offset count total = true → offset + count ≤ total) ∧
    (offset + count ≤ total → guardedRange offset count total = true) := by
  constructor
  · intro h
    simp [guardedRange] at h
    omega
  · intro h
    simp [guardedRange]
    omega

def chooseNumber (flag : Bool) : Nat := if flag then 1 else 0

theorem split_hypothesis (flag : Bool) (h : chooseNumber flag = 1) : flag = true := by
  unfold chooseNumber at h
  split at h
  · assumption
  · contradiction

theorem after_locations : 0 = 0 := by rfl
