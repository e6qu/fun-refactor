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

theorem induction_generalizing (xs : List Nat) (n : Nat) : n = n := by
  induction xs generalizing n with
  | nil => rfl
  | cons head tail ih => rfl

theorem cases_application (flag : Bool) : chooseNumber flag ≤ 1 := by
  cases h : chooseNumber flag with
  | zero => omega
  | succ n =>
    cases flag <;> simp [chooseNumber] at h <;> omega

theorem tuple_proofs (p q : Prop) (hp : p) (hq : q) : p ∧ q := by
  exact ⟨by exact hp, by exact hq⟩

theorem after_tuple_proofs : 1 = 1 := by rfl

theorem wrapped_parenthesized_argument (n : Nat) : n ≤ n + 1 := by
  have bound := Nat.le_add_right n
    (1)
  exact bound

theorem wrapped_arguments (n : Nat) : n ≤ n + 1 := by
  have bound := Nat.le_add_right
    n
    1
  exact bound

def nestedLet (n : Nat) : Nat :=
  if n == 0 then
    let count := n
    count + 1
  else n
