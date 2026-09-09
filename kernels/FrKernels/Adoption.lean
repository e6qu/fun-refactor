namespace FrKernels.Adoption

-- fr:spec src/spec.rs::debt_within_ceiling @ b39b8fe14f4f5ad72ee12dd473c7323c05a8d43c62017e826e408c9ad37856aa
-- fr:signature obligations: usize => obligations: Nat; ceiling: usize => ceiling: Nat; return: bool => return: Bool
def debtWithinCeiling (obligations : Nat) (ceiling : Nat) : Bool :=
  decide (obligations ≤ ceiling)

theorem debt_within_ceiling_iff (obligations ceiling : Nat) :
    debtWithinCeiling obligations ceiling = true ↔ obligations ≤ ceiling := by
  simp [debtWithinCeiling]

theorem equal_ceiling_is_accepted (obligations : Nat) :
    debtWithinCeiling obligations obligations = true := by
  simp [debtWithinCeiling]

theorem reducing_debt_preserves_acceptance
    (after before ceiling : Nat)
    (reduced : after ≤ before)
    (accepted : debtWithinCeiling before ceiling = true) :
    debtWithinCeiling after ceiling = true := by
  simp [debtWithinCeiling] at accepted ⊢
  exact Nat.le_trans reduced accepted

theorem debt_above_ceiling_is_rejected
    (obligations ceiling : Nat)
    (above : ceiling < obligations) :
    debtWithinCeiling obligations ceiling = false := by
  simp [debtWithinCeiling, Nat.not_le.mpr above]

end FrKernels.Adoption
