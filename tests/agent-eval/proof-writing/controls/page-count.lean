constructor
· intro invalid
  rcases invalid with hi | hs
  · simp [page_countModel, hi]
  · simp [page_countModel, hs]
· constructor
  · intro valid
    rcases valid with ⟨hi0, _himax, hs1, _hsmax⟩
    have hi : ¬ items < 0 := by omega
    have hs : ¬ size ≤ 0 := by omega
    have hq0 := Int.tdiv_nonneg hi0 (by omega : 0 ≤ size)
    have hqle := Int.tdiv_le_self size hi0
    by_cases hr : Int.tmod items size = 0
    · simpa [page_countModel, hi, hs, hr] using And.intro hq0 hqle
    · have hipos : 0 < items := by
        by_cases h : 0 < items
        · exact h
        have hz : items = 0 := by omega
        subst items
        simp at hr
      have hsne : size ≠ 1 := by
        intro hz
        subst size
        simp at hr
      have hlt : items.tdiv size < items := by
        rw [Int.tdiv_eq_ediv_of_nonneg hi0]
        exact Int.ediv_lt_self_of_pos_of_ne_one hipos hsne
      have bounds : 0 ≤ items.tdiv size + 1 ∧ items.tdiv size + 1 ≤ items := by omega
      simpa [page_countModel, hi, hs, hr] using bounds
  · decide
