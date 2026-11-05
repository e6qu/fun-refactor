
def allowed (purpose : Nat) : Bool :=
  match purpose with
  | 2 | 3 | 4 => true
  | _ => false

def masked (mode before after : UInt32) : Bool :=
  mode &&& ~~~before == after ||| (mode ^^^ before) == after

theorem admitted (state : Nat) (ready : Bool)
    (values : List Nat) (member : ∀ value ∈ values, value ∈ values) : True := by
  by_cases s0 : state = 0 <;> cases ready <;> simp_all [*]

theorem branches (values : List Nat) (pair : Nat × Nat) : True := by
  induction values with
  | nil => simp
  | cons head tail ih => simp_all
  rcases pair with ⟨left, right⟩
  simp

theorem alternatives (invalid : 0 < 1 ∨ 1 < 2) : True := by
  rcases invalid with left | right
  · trivial
  · trivial
