intro h
rcases h with ⟨ha, hb⟩
change 0 ≤ Int.tdiv a b
exact Int.tdiv_nonneg ha (by omega)
