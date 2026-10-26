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

theorem introduce_arguments (p q : Prop) : p → q → p := by
  intro hp hq
  exact hp

theorem simplify_wildcards (n : Nat) (h : n = 0) : n = 0 := by
  simp only [*]

theorem rewrite_locations (n : Nat) (h : n + 0 = 0) : n + 0 = 0 := by
  rw [Nat.add_zero] at h ⊢
  exact h

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

def appliedLambda : Nat := (fun n : Nat => n + 1) 2

def returnedFunction (flag : Bool) : Nat → Nat :=
  if flag then fun n => n + 1 else fun n => n

def appliedReturnedFunction : Nat := (returnedFunction true) 2

def infixApplication (offset : Nat) (f : Nat → Nat) (n : Nat) : Nat := offset + f n

def wrappedTermBinding (n : Nat) : Nat :=
  let count := Nat.add
    n
    1
  count

def doBindings (n : Nat) : Id Nat := do
  let mut total := n
  let next := Nat.add
    total
    1
  total := next
  let copied ← pure total
  pure copied

def doPattern (n : Nat) : Id Nat := do
  let (left, right) := (n, n + 1)
  pure (left + right)
