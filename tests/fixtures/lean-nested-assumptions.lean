theorem nestedObtain (p q r s : Prop) (h : ((p ∧ q) ∧ r) ∧ s) : p := by
  obtain ⟨⟨⟨hp, _⟩, _⟩, _⟩ := h
  exact hp

theorem nestedRCases (p q r s : Prop) (h : ((p ∧ q) ∧ r) ∧ s) : p := by
  rcases h with ⟨⟨⟨hp, _⟩, _⟩, _⟩
  exact hp

theorem nestedRIntro (p q r s : Prop) : ((p ∧ q) ∧ r) ∧ s → p := by
  rintro ⟨⟨⟨hp, _⟩, _⟩, _⟩
  exact hp

theorem nestedAlternative (h : (True ∨ True) ∧ True) : True := by
  rcases h with ⟨hp | hq, hr⟩ <;> trivial

theorem afterNested : True := by trivial
