import Std

namespace FrPureKernel

def limitsAdmitted (fuel environment nodes depth : Nat) : Bool :=
  1 ≤ fuel && fuel ≤ 256 && environment ≤ 64 && nodes ≤ 4096 && depth ≤ 64

theorem admitted_limits_are_bounded (fuel environment nodes depth : Nat) :
    limitsAdmitted fuel environment nodes depth = true ↔
      1 ≤ fuel ∧ fuel ≤ 256 ∧ environment ≤ 64 ∧ nodes ≤ 4096 ∧ depth ≤ 64 := by
  simp [limitsAdmitted, and_assoc]

inductive Value where
  | unit
  | bool (value : Bool)
  | int (value : Int)
  | string (value : String)
  | tuple (items : List Value)
  | list (items : List Value)
  | record (fields : List (String × Value))
  | option (value : Option Value)
  | result (ok : Bool) (value : Value)
deriving Repr, BEq

inductive Operator where
  | add | sub | mul | div | rem | eq | ne | lt | le | gt | ge | and | or | xor
deriving Repr, BEq

inductive Term where
  | value (value : Value)
  | bound (index : Nat)
  | letE (value body : Term)
  | ite (condition yes no : Term)
  | binary (operator : Operator) (left right : Term)
  | notE (operand : Term)
  | neg (operand : Term)
  | tuple (items : List Term)
  | list (items : List Term)
  | record (fields : List (String × Term))
  | option (value : Option Term)
  | result (ok : Bool) (value : Term)
  | field (value : Term) (name : String)
  | index (value : Term) (index : Nat)
deriving Repr

def minimum : Int := -9223372036854775808
def maximum : Int := 9223372036854775807

def checked (value : Int) : Option Value :=
  if minimum ≤ value ∧ value ≤ maximum then some (.int value) else none

def applyOperator (operator : Operator) (left right : Value) : Option Value :=
  match operator, left, right with
  | .eq, .bool left, .bool right => some (.bool (left == right))
  | .ne, .bool left, .bool right => some (.bool (!(left == right)))
  | .eq, .int left, .int right => some (.bool (left == right))
  | .ne, .int left, .int right => some (.bool (!(left == right)))
  | .eq, left, right => some (.bool (left == right))
  | .ne, left, right => some (.bool (!(left == right)))
  | .and, .bool left, .bool right => some (.bool (left && right))
  | .or, .bool left, .bool right => some (.bool (left || right))
  | .xor, .bool left, .bool right => some (.bool (Bool.xor left right))
  | .add, .int left, .int right => checked (left + right)
  | .sub, .int left, .int right => checked (left - right)
  | .mul, .int left, .int right => checked (left * right)
  | .div, .int left, .int right =>
    if right == 0 || (left == minimum && right == -1) then none
    else checked (Int.tdiv left right)
  | .rem, .int left, .int right =>
    if right == 0 || (left == minimum && right == -1) then none
    else checked (Int.tmod left right)
  | .lt, .int left, .int right => some (.bool (left < right))
  | .le, .int left, .int right => some (.bool (left ≤ right))
  | .gt, .int left, .int right => some (.bool (left > right))
  | .ge, .int left, .int right => some (.bool (left ≥ right))
  | _, _, _ => none

def eval : Nat → List Value → Term → Option Value
  | 0, _, _ => none
  | fuel + 1, environment, term =>
    let run := eval fuel environment
    match term with
    | .value value => some value
    | .bound index => environment[index]?
    | .letE value body => do
      let value ← run value
      eval fuel (value :: environment) body
    | .ite condition yes no => do
      let .bool condition ← run condition | none
      run (if condition then yes else no)
    | .binary operator left right => do
      let left ← run left
      match operator, left with
      | .and, .bool false => pure (.bool false)
      | .or, .bool true => pure (.bool true)
      | _, _ => do
        let right ← run right
        applyOperator operator left right
    | .notE operand => do
      let .bool value ← run operand | none
      pure (.bool (!value))
    | .neg operand => do
      let .int value ← run operand | none
      checked (-value)
    | .tuple items => return .tuple (← items.mapM run)
    | .list items => return .list (← items.mapM run)
    | .record fields => do
      let fields ← fields.mapM fun (name, term) => do
        let value ← run term
        pure (name, value)
      pure (.record fields)
    | .option none => some (.option none)
    | .option (some value) => return .option (some (← run value))
    | .result ok value => return .result ok (← run value)
    | .field value name => do
      let .record fields ← run value | none
      fields.findSome? fun (field, value) => if field == name then some value else none
    | .index value index => do
      let value ← run value
      match value with
      | .tuple items | .list items => items[index]?
      | _ => none

def resolve : List String → String → Option Nat
  | [], _ => none
  | head :: tail, name => if head == name then some 0 else (resolve tail name).map Nat.succ

theorem resolution_selects_the_shadowing_binding (name : String) (names : List String) :
    resolve (name :: names) name = some 0 := by simp [resolve]

theorem resolution_lifts_an_unshadowed_binding (head name : String) (names : List String)
    (different : head ≠ name) :
    resolve (head :: names) name = (resolve names name).map Nat.succ := by
  simp [resolve, different]

theorem inserted_binding_occupies_zero (fuel : Nat) (value : Value) (environment : List Value) :
    eval (fuel + 1) (value :: environment) (.bound 0) = some value := by rfl

theorem lifted_indices_avoid_capture (fuel index : Nat) (value : Value) (environment : List Value) :
    eval (fuel + 1) (value :: environment) (.bound (index + 1)) =
      eval (fuel + 1) environment (.bound index) := by rfl

theorem literal_binding_substitution (fuel : Nat) (value : Value) (body : Term) (environment : List Value) :
    eval (fuel + 2) environment (.letE (.value value) body) =
      eval (fuel + 1) (value :: environment) body := by rfl

theorem evaluation_is_deterministic (fuel : Nat) (term : Term) (environment : List Value)
    (left right : Value) (a : eval fuel environment term = some left)
    (b : eval fuel environment term = some right) : left = right := by
  rw [a] at b
  exact Option.some.inj b

theorem successful_arithmetic_is_bounded (value : Int) (result : Value)
    (accepted : checked value = some result) : minimum ≤ value ∧ value ≤ maximum := by
  unfold checked at accepted
  split at accepted
  · assumption
  · contradiction

theorem grouping_preserves_operator_precedence :
    eval 64 [] (.binary .add (.value (.int 2))
      (.binary .mul (.value (.int 3)) (.value (.int 4)))) = some (.int 14) ∧
    eval 64 [] (.binary .mul (.binary .add (.value (.int 2)) (.value (.int 3)))
      (.value (.int 4))) = some (.int 20) := by constructor <;> rfl

theorem multiplication_is_evaluated_before_addition
    (a b c product total : Int)
    (multiply : checked (b * c) = some (.int product))
    (add : checked (a + product) = some (.int total)) :
    eval 4 [] (.binary .add (.value (.int a))
      (.binary .mul (.value (.int b)) (.value (.int c)))) = some (.int total) := by
  simp [eval, applyOperator, multiply, add]

theorem unused_false_conjunction_is_not_evaluated (fuel : Nat) (right : Term)
    (environment : List Value) :
    eval (fuel + 2) environment (.binary .and (.value (.bool false)) right) =
      some (.bool false) := by rfl

theorem unused_true_disjunction_is_not_evaluated (fuel : Nat) (right : Term)
    (environment : List Value) :
    eval (fuel + 2) environment (.binary .or (.value (.bool true)) right) =
      some (.bool true) := by rfl

theorem substitution_under_shadowing_preserves_the_outer_binding
    (fuel : Nat) (outer inner : Value) (environment : List Value) :
    eval (fuel + 3) environment
      (.letE (.value outer) (.letE (.value inner) (.bound 1))) = some outer := by rfl

end FrPureKernel

namespace FrRustScalar
open FrPureKernel

def minimum (signed : Bool) : Int := if signed then -9223372036854775808 else 0
def maximum (signed : Bool) : Int := if signed then 9223372036854775807 else 18446744073709551615

def checked (signed : Bool) (value : Int) : Option Value :=
  if minimum signed ≤ value ∧ value ≤ maximum signed then some (.int value) else none

def applyOperator (signed : Bool) (operator : Operator) (left right : Value) : Option Value :=
  match operator, left, right with
  | .add, .int left, .int right => checked signed (left + right)
  | .sub, .int left, .int right => checked signed (left - right)
  | .mul, .int left, .int right => checked signed (left * right)
  | .div, .int left, .int right =>
    if right == 0 || (signed && left == minimum true && right == -1) then none
    else checked signed (Int.tdiv left right)
  | .rem, .int left, .int right =>
    if right == 0 || (signed && left == minimum true && right == -1) then none
    else checked signed (Int.tmod left right)
  | _, _, _ => FrPureKernel.applyOperator operator left right

def eval (signed : Bool) : Nat → List Value → Term → Option Value
  | 0, _, _ => none
  | fuel + 1, environment, term =>
    let run := eval signed fuel environment
    match term with
    | .value (.int value) => checked signed value
    | .value (.bool value) => some (.bool value)
    | .bound index => environment[index]?
    | .letE value body => do
      let value ← run value
      eval signed fuel (value :: environment) body
    | .ite condition yes no => do
      let .bool condition ← run condition | none
      run (if condition then yes else no)
    | .binary operator left right => do
      let left ← run left
      match operator, left with
      | .and, .bool false => pure (.bool false)
      | .or, .bool true => pure (.bool true)
      | _, _ => do
        let right ← run right
        applyOperator signed operator left right
    | .notE operand => do
      let .bool value ← run operand | none
      pure (.bool (!value))
    | .neg operand => do
      let .int value ← run operand | none
      checked signed (-value)
    | _ => none

theorem branch_true {signed : Bool} {fuel : Nat} {environment : List Value}
    {condition yes no : Term}
    (selected : eval signed fuel environment condition = some (.bool true)) :
    eval signed (fuel + 1) environment (.ite condition yes no) =
      eval signed fuel environment yes := by
  rw [eval, selected] <;> rfl

theorem branch_false {signed : Bool} {fuel : Nat} {environment : List Value}
    {condition yes no : Term}
    (selected : eval signed fuel environment condition = some (.bool false)) :
    eval signed (fuel + 1) environment (.ite condition yes no) =
      eval signed fuel environment no := by
  rw [eval, selected] <;> rfl

theorem binding_checked {signed : Bool} {fuel : Nat} {environment : List Value}
    {value body : Term} {result : Value}
    (checkedValue : eval signed fuel environment value = some result) :
    eval signed (fuel + 1) environment (.letE value body) =
      eval signed fuel (result :: environment) body := by
  rw [eval, checkedValue] <;> rfl

theorem disjunction {signed : Bool} {fuel : Nat} {environment : List Value}
    {left right : Term} {a b : Bool}
    (leftChecked : eval signed fuel environment left = some (.bool a))
    (rightChecked : eval signed fuel environment right = some (.bool b)) :
    eval signed (fuel + 1) environment (.binary .or left right) = some (.bool (a || b)) := by
  rw [eval, leftChecked]
  cases a with
  | false => rw [rightChecked] <;> rfl
  | true => rfl

theorem successful_arithmetic_is_bounded (signed : Bool) (value : Int) (result : Value)
    (accepted : checked signed value = some result) :
    minimum signed ≤ value ∧ value ≤ maximum signed := by
  unfold checked at accepted
  split at accepted
  · assumption
  · contradiction

theorem unsigned_subtraction_requires_order (left right : Int) (result : Value)
    (accepted : applyOperator false .sub (.int left) (.int right) = some result) :
    right ≤ left := by
  have bound := successful_arithmetic_is_bounded false (left - right) result accepted
  simp [minimum] at bound
  omega

theorem signed_division_overflow_refuses :
    applyOperator true .div (.int (-9223372036854775808)) (.int (-1)) = none := by rfl

theorem signed_remainder_overflow_refuses :
    applyOperator true .rem (.int (-9223372036854775808)) (.int (-1)) = none := by rfl

end FrRustScalar
