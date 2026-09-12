namespace FrKernels.SemanticIntent

-- fr:spec src/project/semantic_intent.rs::semantic_intent_admitted @ bf31f22d028f437eced0d4c2f3c737247fe563e5a7f6a800bf7a5fa3bd58131b
-- fr:signature schema_matches: bool => schemaMatches: Bool; base_well_formed: bool => baseWellFormed: Bool; base_matches: bool => baseMatches: Bool; source_free: bool => sourceFree: Bool; operation_count: usize => operationCount: Nat; return: bool => return: Bool
def admitted
    (schemaMatches : Bool)
    (baseWellFormed : Bool)
    (baseMatches : Bool)
    (sourceFree : Bool)
    (operationCount : Nat) : Bool :=
  schemaMatches && baseWellFormed && baseMatches && sourceFree &&
    decide (1 ≤ operationCount ∧ operationCount ≤ 64)

theorem admitted_iff
    (schemaMatches baseWellFormed baseMatches sourceFree : Bool) (operationCount : Nat) :
    admitted schemaMatches baseWellFormed baseMatches sourceFree operationCount = true ↔
      schemaMatches = true ∧ baseWellFormed = true ∧ baseMatches = true ∧
        sourceFree = true ∧ 1 ≤ operationCount ∧ operationCount ≤ 64 := by
  cases schemaMatches <;> cases baseWellFormed <;> cases baseMatches <;> cases sourceFree <;>
    simp [admitted]

theorem admitted_requires_matching_base
    (schemaMatches baseWellFormed baseMatches sourceFree : Bool) (operationCount : Nat)
    (accepted : admitted schemaMatches baseWellFormed baseMatches sourceFree operationCount = true) :
    baseMatches = true := by
  exact (admitted_iff schemaMatches baseWellFormed baseMatches sourceFree operationCount).mp
    accepted |>.2.2.1

theorem admitted_bounds_operations
    (schemaMatches baseWellFormed baseMatches sourceFree : Bool) (operationCount : Nat)
    (accepted : admitted schemaMatches baseWellFormed baseMatches sourceFree operationCount = true) :
    1 ≤ operationCount ∧ operationCount ≤ 64 := by
  exact (admitted_iff schemaMatches baseWellFormed baseMatches sourceFree operationCount).mp
    accepted |>.2.2.2.2

-- fr:spec src/project/semantic_intent.rs::semantic_locator_bounded @ 3edc1174f27f293eec4e9e70e955d2e53f35db627b643ca88959156a2a0d36d7
-- fr:signature steps: usize => steps: Nat; return: bool => return: Bool
def locatorBounded (steps : Nat) : Bool := decide (1 ≤ steps ∧ steps ≤ 64)

theorem locator_bounded_iff (steps : Nat) :
    locatorBounded steps = true ↔ 1 ≤ steps ∧ steps ≤ 64 := by
  simp [locatorBounded]

theorem empty_locator_refused : locatorBounded 0 = false := by decide
theorem maximum_locator_accepted : locatorBounded 64 = true := by decide
theorem oversized_locator_refused : locatorBounded 65 = false := by decide

-- Categories are 0 type, 1 statement, 2 expression and 3 template. Kind codes follow the
-- corresponding public semantic catalog. Operations follow fr-semantic-intent-1 catalog order.
-- fr:spec src/project/semantic_intent.rs::semantic_intent_operation_allowed @ df48d7ef7345e90a06032f752e5ead3ec38ccaee8500fc6cef631a47afcf8394
-- fr:signature operation: usize => operation: Nat; category: usize => category: Nat; kind: usize => kind: Nat; return: bool => return: Bool
def operationAllowed (operation : Nat) (category : Nat) (kind : Nat) : Bool :=
  match operation with
  | 0 => decide (category = 2 ∧ kind = 0)
  | 1 => decide (category = 2 ∧ kind = 1)
  | 2 => decide (category = 2 ∧ kind = 2)
  | 3 => decide (category = 2 ∧ kind = 3)
  | 4 => decide (category = 2 ∧ kind = 5)
  | 5 => decide (category = 2 ∧ kind = 6)
  | 6 => decide (category = 2 ∧ kind = 13)
  | 7 => decide (category = 2 ∧ kind = 9)
  | 8 => decide (category = 2 ∧ kind = 10)
  | 9 => decide (category = 3 ∧ kind = 0)
  | 10 => decide (category = 1 ∧ kind = 17)
  | _ => false

theorem integer_intent_has_exact_target (category kind : Nat) :
    operationAllowed 0 category kind = true ↔ category = 2 ∧ kind = 0 := by
  simp [operationAllowed]

theorem comment_intent_has_exact_target (category kind : Nat) :
    operationAllowed 10 category kind = true ↔ category = 1 ∧ kind = 17 := by
  simp [operationAllowed]

def operationTargets : List (Nat × Nat) :=
  [(2, 0), (2, 1), (2, 2), (2, 3), (2, 5), (2, 6), (2, 13), (2, 9), (2, 10),
   (3, 0), (1, 17)]

theorem operation_targets_are_unique : operationTargets.Nodup := by decide

structure LocatorCandidate where
  locator : List Nat
  node : Nat
deriving DecidableEq

def resolveLocator (candidates : List LocatorCandidate) (locator : List Nat) : Option Nat :=
  match candidates.filter (fun candidate => candidate.locator == locator) with
  | [candidate] => some candidate.node
  | _ => none

theorem locator_resolution_deterministic (candidates : List LocatorCandidate) (locator : List Nat)
    (left right : Nat) (leftResolved : resolveLocator candidates locator = some left)
    (rightResolved : resolveLocator candidates locator = some right) : left = right := by
  exact Option.some.inj (leftResolved.symm.trans rightResolved)

theorem locator_resolves_exact_singleton (candidate : LocatorCandidate) :
    resolveLocator [candidate] candidate.locator = some candidate.node := by
  simp [resolveLocator]

structure ScalarNode where
  category : Nat
  kind : Nat
  scalar : Nat
  children : List Nat
deriving DecidableEq

def ScalarNode.withScalar (node : ScalarNode) (scalar : Nat) : ScalarNode :=
  { node with scalar }

theorem scalar_edit_preserves_category (node : ScalarNode) (scalar : Nat) :
    (node.withScalar scalar).category = node.category := by rfl

theorem scalar_edit_preserves_kind (node : ScalarNode) (scalar : Nat) :
    (node.withScalar scalar).kind = node.kind := by rfl

theorem scalar_edit_preserves_children (node : ScalarNode) (scalar : Nat) :
    (node.withScalar scalar).children = node.children := by rfl

structure ScalarIntent where
  operation : Nat
  before : Nat
  after : Nat
deriving DecidableEq

structure SemanticReplace where
  category : Nat
  kind : Nat
  before : Nat
  value : ScalarNode
deriving DecidableEq

def intentAllowed (node : ScalarNode) (intent : ScalarIntent) : Bool :=
  operationAllowed intent.operation node.category node.kind &&
    decide (node.scalar = intent.before ∧ intent.before ≠ intent.after)

def interpret (node : ScalarNode) (intent : ScalarIntent) : Option ScalarNode :=
  if intentAllowed node intent then some (node.withScalar intent.after) else none

def compile (node : ScalarNode) (intent : ScalarIntent) : Option SemanticReplace :=
  if intentAllowed node intent then
    some {
      category := node.category
      kind := node.kind
      before := intent.before
      value := node.withScalar intent.after
    }
  else none

def applyReplace (node : ScalarNode) (change : SemanticReplace) : Option ScalarNode :=
  if node.category = change.category ∧ node.kind = change.kind ∧ node.scalar = change.before then
    some change.value
  else none

theorem compiler_refines_direct_interpretation (node : ScalarNode) (intent : ScalarIntent) :
    (compile node intent).bind (applyReplace node) = interpret node intent := by
  unfold compile interpret
  split <;> simp_all [applyReplace, intentAllowed]

def interpretAll (node : ScalarNode) (intents : List ScalarIntent) : Option ScalarNode :=
  intents.foldl (fun current intent => current.bind (fun value => interpret value intent)) (some node)

def compileAndApplyAll (node : ScalarNode) (intents : List ScalarIntent) : Option ScalarNode :=
  intents.foldl (fun current intent => current.bind (fun value =>
    (compile value intent).bind (applyReplace value))) (some node)

theorem foldl_none_absorbing {α β : Type} (values : List α)
    (step : Option β → α → Option β) (absorbing : ∀ value, step none value = none) :
    values.foldl step none = none := by
  induction values with
  | nil => rfl
  | cons head tail induction =>
      simp only [List.foldl_cons, absorbing]
      exact induction

theorem ordered_compiler_refines_interpretation (node : ScalarNode) (intents : List ScalarIntent) :
    compileAndApplyAll node intents = interpretAll node intents := by
  induction intents generalizing node with
  | nil => rfl
  | cons intent rest induction =>
      simp only [compileAndApplyAll, interpretAll, List.foldl_cons, Option.bind_some]
      rw [compiler_refines_direct_interpretation]
      cases interpreted : interpret node intent with
      | none =>
          have leftNone := foldl_none_absorbing rest
            (fun current intent => current.bind (fun value =>
              (compile value intent).bind (applyReplace value))) (by simp)
          have rightNone := foldl_none_absorbing rest
            (fun current intent => current.bind (fun value => interpret value intent)) (by simp)
          rw [leftNone, rightNone]
      | some next =>
          simpa [compileAndApplyAll, interpretAll] using induction next

end FrKernels.SemanticIntent
