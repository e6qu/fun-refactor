namespace LibraryHeaders

def choose : Bool → Bool
  | value => value

protected theorem nonnegative : ∀ {α : Int}, 0 ≤ α → 0 ≤ α
  | _, hypothesis => hypothesis

theorem arrowProof {p q : Prop} : p ∧ q → p := by
  intro h
  exact h.1

end LibraryHeaders
