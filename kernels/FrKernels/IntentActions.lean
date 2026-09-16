import FrKernels.Workflow

namespace FrKernels.IntentActions

-- fr:spec src/project/agent_actions.rs::intent_action_purpose_allowed @ 7cee75af68d7396e74be55addec3e0c0ad1bc3943cb0ef95edd8fc95b621367f
-- fr:signature purpose: usize => purpose: Nat; operation: usize => operation: Nat; return: bool => return: Bool
def purposeAllowed (purpose : Nat) (operation : Nat) : Bool :=
  (purpose == 2 && (operation ≤ 2 || operation == 7 || operation == 11)) ||
    (purpose == 3 && operation == 3) ||
    (purpose == 4 && ((4 ≤ operation && operation ≤ 5) || (8 ≤ operation && operation ≤ 9))) ||
    (purpose ≤ 4 && (operation == 6 || operation == 10))

theorem task_requires_change (purpose : Nat) : purposeAllowed purpose 0 = true ↔ purpose = 2 := by
  simp [purposeAllowed]

theorem migration_requires_migrate (purpose : Nat) : purposeAllowed purpose 3 = true ↔ purpose = 3 := by
  simp [purposeAllowed]

theorem formal_plan_requires_prove (purpose : Nat) : purposeAllowed purpose 4 = true ↔ purpose = 4 := by
  simp [purposeAllowed]

theorem proof_requires_prove (purpose : Nat) : purposeAllowed purpose 5 = true ↔ purpose = 4 := by
  simp [purposeAllowed]

-- fr:spec src/project/agent_actions.rs::intent_review_complete @ c26e66b3103f818c5cbcca8a29c4bf3b81a466000f535f364f70bbc4b5dcbe69
-- fr:signature target_count: usize => targetCount: Nat; evidence_count: usize => evidenceCount: Nat; checks_declared: bool => checksDeclared: Bool; writable: bool => writable: Bool; proof_required: bool => proofRequired: Bool; proof_checked: bool => proofChecked: Bool; implementation_requested: bool => implementationRequested: Bool; implementation_evidence: bool => implementationEvidence: Bool; diff_complete: bool => diffComplete: Bool; return: bool => return: Bool
def complete (targetCount : Nat) (evidenceCount : Nat)
    (checksDeclared : Bool) (writable : Bool) (proofRequired : Bool) (proofChecked : Bool)
    (implementationRequested : Bool) (implementationEvidence : Bool) (diffComplete : Bool) : Bool :=
  (1 ≤ targetCount && targetCount ≤ 32 && evidenceCount == targetCount - 1) &&
    (!writable || checksDeclared) && (!proofRequired || proofChecked) &&
    (!implementationRequested || implementationEvidence) && diffComplete

theorem complete_binds_every_target
    (targets evidence : Nat) (checks writable required checked requested correspondence diff : Bool)
    (accepted : complete targets evidence checks writable required checked requested correspondence diff = true) :
    1 ≤ targets ∧ targets ≤ 32 ∧ targets = evidence + 1 := by
  simp [complete] at accepted
  omega

theorem missing_proof_refuses
    (targets evidence : Nat) (checks writable requested correspondence diff : Bool) :
    complete targets evidence checks writable true false requested correspondence diff = false := by
  simp [complete]

theorem implementation_claim_requires_separate_evidence
    (targets evidence : Nat) (checks writable required checked diff : Bool) :
    complete targets evidence checks writable required checked true false diff = false := by
  simp [complete]

-- fr:spec src/project/agent_actions.rs::intent_review_mode @ f638125049f787cc2cb89bbe96c9f3e9f1807cda1c09ff1fd0a9e0dd926939de
-- fr:signature purpose: usize => purpose: Nat; operation: usize => operation: Nat; complete: bool => completeReview: Bool; writable: bool => writable: Bool; write: bool => write: Bool; basis_supplied: bool => basisSupplied: Bool; basis_matches: bool => basisMatches: Bool; return: usize => return: Nat
def mode (purpose : Nat) (operation : Nat)
    (completeReview : Bool) (writable : Bool) (write : Bool) (basisSupplied : Bool) (basisMatches : Bool) : Nat :=
  if !purposeAllowed purpose operation || !completeReview then 2
  else if !write && !basisSupplied then 0
  else if writable && write && basisSupplied && basisMatches then 1
  else 2

theorem execution_requires_unchanged_complete_writable_review
    (purpose operation : Nat) (completeReview writable write supplied matching : Bool)
    (accepted : mode purpose operation completeReview writable write supplied matching = 1) :
    purposeAllowed purpose operation = true ∧ completeReview = true ∧ writable = true ∧
      write = true ∧ supplied = true ∧ matching = true := by
  cases h : purposeAllowed purpose operation <;>
    cases completeReview <;> cases writable <;> cases write <;> cases supplied <;> cases matching <;>
      simp_all [mode]

theorem read_only_review_never_executes
    (purpose operation : Nat) (completeReview write supplied matching : Bool) :
    mode purpose operation completeReview false write supplied matching ≠ 1 := by
  cases h : purposeAllowed purpose operation <;>
    cases completeReview <;> cases write <;> cases supplied <;> cases matching <;> simp_all [mode]

theorem accepted_delivery_finishes_applied
    (purpose operation : Nat) (completeReview writable write supplied matching original reversal patch : Bool)
    (accepted : mode purpose operation completeReview writable write supplied matching = 1) :
    FrKernels.Workflow.run .planned (FrKernels.Workflow.lifecycle original reversal patch) = some .applied := by
  obtain ⟨_, rfl, rfl, rfl, rfl, rfl⟩ :=
    execution_requires_unchanged_complete_writable_review purpose operation completeReview writable write supplied matching accepted
  exact FrKernels.Workflow.generated_lifecycle_finishes_applied original reversal patch

end FrKernels.IntentActions
