namespace FrKernels.AgentGuide

def purposeMatches (purpose route : Nat) : Bool :=
  match route with
  | 0 => decide (purpose ≤ 4)
  | 1 => decide (purpose ≤ 2)
  | 2 | 3 | 4 | 5 | 6 => purpose == 2
  | 7 => purpose == 3
  | 8 | 9 => purpose == 4
  | _ => false

-- fr:spec src/project/agent_guide.rs::agent_guide_route_admitted @ d7742594b8cb02e31f964a4650e3c3ec02299a60a2996f79216b0651ad318873
-- fr:signature purpose: usize => purpose: Nat; language_class: usize => languageClass: Nat; target_kind: usize => targetKind: Nat; route: usize => route: Nat; supported: bool => supported: Bool; source_required: bool => sourceRequired: Bool; source_allowed: bool => sourceAllowed: Bool; proof_expectation: usize => proofExpectation: Nat; return: bool => return: Bool
def routeAdmitted
    (purpose : Nat) (languageClass : Nat) (targetKind : Nat) (route : Nat)
    (supported : Bool) (sourceRequired : Bool) (sourceAllowed : Bool)
    (proofExpectation : Nat) : Bool :=
  purposeMatches purpose route && decide (languageClass ≤ 1) &&
    decide (targetKind ≤ 2) && supported && (!sourceRequired || sourceAllowed) &&
    decide (proofExpectation ≤ 1)

theorem admission_requires_live_support
    (accepted : routeAdmitted purpose languageClass targetKind route
      supported sourceRequired sourceAllowed proofExpectation = true) :
    supported = true := by
  simp only [routeAdmitted, Bool.and_eq_true] at accepted
  exact accepted.1.1.2

theorem admission_respects_source_permission
    (accepted : routeAdmitted purpose languageClass targetKind route
      supported true sourceAllowed proofExpectation = true) :
    sourceAllowed = true := by
  simp [routeAdmitted] at accepted
  exact accepted.1.2

theorem admission_requires_matching_purpose
    (accepted : routeAdmitted purpose languageClass targetKind route
      supported sourceRequired sourceAllowed proofExpectation = true) :
    purposeMatches purpose route = true := by
  simp only [routeAdmitted, Bool.and_eq_true] at accepted
  exact accepted.1.1.1.1.1

theorem implementation_claim_is_never_admitted
    (purpose languageClass targetKind route : Nat)
    (supported sourceRequired sourceAllowed : Bool) :
    routeAdmitted purpose languageClass targetKind route
      supported sourceRequired sourceAllowed 2 = false := by
  simp [routeAdmitted]

-- fr:spec src/project/agent_guide.rs::agent_guide_step @ 60909d7cda92172ebed49180173f2c54b53df30541196c6f4ec2ab099899c578
-- fr:signature state: usize => state: Nat; action: usize => action: Nat; ready: bool => ready: Bool; complete_review: bool => completeReview: Bool; basis_matches: bool => basisMatches: Bool; return: usize => return: Nat
def step (state : Nat) (action : Nat) (ready : Bool) (completeReview : Bool) (basisMatches : Bool) : Nat :=
  if state == 0 && action == 0 && ready then 1
  else if state == 1 && action == 1 && ready then 2
  else if state == 2 && action == 2 && ready && completeReview then 3
  else if state == 3 && action == 3 && ready && completeReview && basisMatches then 4
  else 5

theorem execute_requires_complete_unchanged_review
    (state action : Nat) (ready completeReview basisMatches : Bool) :
    step state action ready completeReview basisMatches = 4 ↔
      state = 3 ∧ action = 3 ∧ ready = true ∧
        completeReview = true ∧ basisMatches = true := by
  by_cases s0 : state = 0 <;> by_cases s1 : state = 1 <;>
    by_cases s2 : state = 2 <;> by_cases s3 : state = 3 <;>
    by_cases a0 : action = 0 <;> by_cases a1 : action = 1 <;>
    by_cases a2 : action = 2 <;> by_cases a3 : action = 3 <;>
    cases ready <;> cases completeReview <;> cases basisMatches <;>
    simp_all [step]

theorem draft_cannot_execute (ready completeReview basisMatches : Bool) :
    step 0 3 ready completeReview basisMatches = 5 := by
  simp [step]

theorem incomplete_preview_cannot_be_reviewed (ready basisMatches : Bool) :
    step 2 2 ready false basisMatches = 5 := by
  simp [step]

-- fr:spec src/project/agent_guide.rs::agent_guide_binding_admitted @ 51cc53241b240ba7e3ba1330b61e1d2a88e23463b042a24acd9416e8feed1c61
-- fr:signature expected_fields: usize => expectedFields: Nat; supplied_fields: usize => suppliedFields: Nat; names_match: bool => namesMatch: Bool; values_bounded: bool => valuesBounded: Bool; execution_disabled: bool => executionDisabled: Bool; basis_matches: bool => basisMatches: Bool; return: bool => return: Bool
def bindingAdmitted (expectedFields suppliedFields : Nat) (namesMatch valuesBounded
    executionDisabled basisMatches : Bool) : Bool :=
  decide (expectedFields ≤ 32) && decide (expectedFields = suppliedFields) && namesMatch &&
    valuesBounded && executionDisabled && basisMatches

theorem binding_requires_exact_named_bounded_inputs
    (accepted : bindingAdmitted expectedFields suppliedFields namesMatch valuesBounded
      executionDisabled basisMatches = true) :
    (((((expectedFields ≤ 32 ∧ expectedFields = suppliedFields) ∧ namesMatch = true) ∧
      valuesBounded = true) ∧ executionDisabled = true) ∧ basisMatches = true) := by
  simpa [bindingAdmitted, Bool.and_eq_true] using accepted

theorem changed_guide_cannot_bind
    (expectedFields suppliedFields : Nat) (namesMatch valuesBounded executionDisabled : Bool) :
    bindingAdmitted expectedFields suppliedFields namesMatch valuesBounded executionDisabled false = false := by
  simp [bindingAdmitted]

-- fr:spec src/project/agent_guide.rs::agent_guide_delivery_admitted @ 60e779b03d67c7e7d83c98bf3a2b80dacccff1f961e0237e145474d4ea240922
-- fr:signature purpose: usize => purpose: Nat; action_count: usize => actionCount: Nat; report_count: usize => reportCount: Nat; review_count: usize => reviewCount: Nat; route_admitted: bool => routeAccepted: Bool; guide_matches: bool => guideMatches: Bool; review_complete: bool => reviewComplete: Bool; return: bool => return: Bool
def deliveryAdmitted (purpose actionCount reportCount reviewCount : Nat)
    (routeAccepted guideMatches reviewComplete : Bool) : Bool :=
  purpose == 2 && decide (1 ≤ actionCount) && decide (actionCount ≤ 16) &&
    decide (reportCount = actionCount) && decide (reviewCount = 1) && routeAccepted &&
    guideMatches && reviewComplete

theorem delivery_requires_one_complete_unchanged_review
    (accepted : deliveryAdmitted purpose actionCount reportCount reviewCount
      routeAccepted guideMatches reviewComplete = true) :
    purpose = 2 ∧ 1 ≤ actionCount ∧ actionCount ≤ 16 ∧ reportCount = actionCount ∧
      reviewCount = 1 ∧ routeAccepted = true ∧ guideMatches = true ∧ reviewComplete = true := by
  simpa [deliveryAdmitted, Bool.and_eq_true, and_assoc] using accepted

theorem stale_guide_cannot_deliver
    (purpose actionCount reportCount reviewCount : Nat) (routeAccepted reviewComplete : Bool) :
    deliveryAdmitted purpose actionCount reportCount reviewCount
      routeAccepted false reviewComplete = false := by
  simp [deliveryAdmitted]

end FrKernels.AgentGuide
