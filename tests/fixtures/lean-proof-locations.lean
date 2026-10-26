import Std

theorem simplify_hypothesis (n : Nat) (h : n + 0 = 0) : n = 0 := by
  simp only [Nat.add_zero] at h
  exact h

theorem simplify_everywhere (n : Nat) (h : n + 0 = 0) : n + 0 = 0 := by
  simp only [Nat.add_zero] at h ⊢
  exact h

theorem transport_equality (a b : Nat) (h : a + b = 0) : b + a = 0 := by
  exact Nat.add_comm a b ▸ h
