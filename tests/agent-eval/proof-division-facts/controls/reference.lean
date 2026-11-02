intro h
rcases h with ⟨ha, hb⟩
change Int.tdiv a b + Int.tmod a b ≤ a
have quotient_nonneg : 0 ≤ Int.tdiv a b := Int.tdiv_nonneg ha (by omega)
have division_identity := Int.tmod_add_mul_tdiv a b
have product_bound : Int.tdiv a b ≤ b * Int.tdiv a b := by
  simpa using Int.mul_le_mul_of_nonneg_right hb quotient_nonneg
omega
