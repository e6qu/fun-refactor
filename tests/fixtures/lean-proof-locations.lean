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

def negativeBindingBody (n : Int) : Int :=
  let value := n;
  -value

def negativeDeclaration (n : Int) : Int :=
  -n

def subtractionContinuation (n : Int) : Int :=
  let value := n
  - 1
  value

example : negativeBindingBody 3 = -3 := by rfl
example : negativeDeclaration 3 = -3 := by rfl
example : subtractionContinuation 3 = 2 := by rfl

def appliedLambda : Nat := (fun n : Nat => n + 1) 2

def appliedTypedLambda : Nat := (fun n m : Nat => n + m) 2 3

def typedArrowLambda : Nat := (fun f : Nat → Nat => f 1) (fun n => n)

def projectionArgument (f : Nat → Nat) (pair : Nat × Nat) : Nat := f pair.1

def subscriptArgument (f : Nat → Nat) (values : Array Nat) : Nat := f values[0]!

def resultProjection (f : Nat → Nat × Nat) (n : Nat) : Nat := (f n).1

def tailLambda : Nat := (fun f : Nat → Nat => f 2) fun n => n + 1

def tailDo : Nat := Id.run do pure 1

theorem parenthesizedProof : 0 = 0 := id (by rfl)

theorem afterTailProof : 1 = 1 := by rfl

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

def conjoinInputs (left right : Bool) : Bool := left && right

def printTruthTable : IO Unit := do
  for left in [false, true] do
    for right in [false, true] do
      IO.println (conjoinInputs left right)

def printBracketedCounter : IO Unit := do
  let mut i : Int := 0
  while (i < 3) do
    i := i + 1

def printCounter : IO Unit := do
  let mut i : Int := 0
  while i < 3 do
    i := i + 1

def printUnlessThree (i : Int) : IO Unit := do
  unless i = 3 do
    IO.println i

def elsePrefix (n elseValue : Int) : Int :=
  let value := n
  elseValue + value

def catchPrefix (n catchValue : Int) : Int :=
  let value := n
  catchValue + value

def finallyPrefix (n finallyValue : Int) : Int :=
  let value := n
  finallyValue + value

namespace KeywordNames

def «if» : Nat := 1
def «by» : Nat := 2

def quotedIfName : Lean.Name := ``«if»

end KeywordNames

example : KeywordNames.if = 1 := by rfl
example : KeywordNames.by = 2 := by rfl

theorem obtain_many (p q r s t : Prop) (h : p ∧ q ∧ r ∧ s ∧ t) : p := by
  obtain ⟨hp, hq, hr, hs, ht⟩ := h
  exact hp

def takePresent (value : Option Nat) : Option Nat := do
  let .some n ← some value | none
  pure n

theorem split_bool_after_locations (p q : Bool) :
    Unit → (p || q) = true → (p || q) = true := by
  intro origin h
  simp only [Bool.or_eq_true] at h ⊢
  rcases h with h | h
  · exact Or.inl h
  · exact Or.inr h

theorem changeComparison (left right : Nat) : left ≤ max left right := by
  change left ≤ max left right
  exact Nat.le_max_left _ _

theorem changeBeforeSplit (head value : Nat) (tail : List Nat)
    (member : value ∈ head :: tail) : value = head ∨ value ∈ tail := by
  change value = head ∨ value ∈ tail
  rcases List.mem_cons.mp member with same | rest
  · exact Or.inl same
  · exact Or.inr rest

theorem showComparison (n : Int) (h : 1 ≤ n) : 1 ≤ n := by
  exact (show (1 : Int) ≤ n by omega)

theorem showFrom (n : Int) (h : 1 ≤ n) : 1 ≤ n :=
  show (1 : Int) ≤ n from h

theorem showThenTactic (n : Int) (h : 1 ≤ n) : 1 ≤ n := by
  show (1 : Int) ≤ n
  exact h

theorem afterShow : True := by trivial
