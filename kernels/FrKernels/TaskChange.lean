import FrKernels.Workflow

namespace FrKernels.TaskChange

-- fr:spec src/project/task_change.rs::task_change_mode @ 8a74eb4e61483acb57f3a2c5a8f3389dd93f104dc86796ad71be89f678331d84
-- fr:signature complete_review: bool => completeReview: Bool; write: bool => write: Bool; basis_supplied: bool => basisSupplied: Bool; basis_matches: bool => basisMatches: Bool; return: usize => return: Nat
def mode
    (completeReview : Bool)
    (write : Bool)
    (basisSupplied : Bool)
    (basisMatches : Bool) : Nat :=
  match completeReview, write, basisSupplied, basisMatches with
  | true, false, false, _ => 0
  | true, true, true, true => 1
  | _, _, _, _ => 2

theorem preview_requires_complete_review
    (completeReview write basisSupplied basisMatches : Bool)
    (accepted : mode completeReview write basisSupplied basisMatches = 0) :
    completeReview = true ∧ write = false ∧ basisSupplied = false := by
  cases completeReview <;> cases write <;> cases basisSupplied <;> cases basisMatches <;>
    simp_all [mode]

theorem execution_requires_matching_review
    (completeReview write basisSupplied basisMatches : Bool)
    (accepted : mode completeReview write basisSupplied basisMatches = 1) :
    completeReview = true ∧ write = true ∧ basisSupplied = true ∧ basisMatches = true := by
  cases completeReview <;> cases write <;> cases basisSupplied <;> cases basisMatches <;>
    simp_all [mode]

theorem accepted_execution_finishes_applied
    (completeReview write basisSupplied basisMatches exerciseReversal deliverPatch : Bool)
    (accepted : mode completeReview write basisSupplied basisMatches = 1) :
    FrKernels.Workflow.run .planned
      (FrKernels.Workflow.lifecycle exerciseReversal deliverPatch) = some .applied := by
  obtain ⟨rfl, rfl, rfl, rfl⟩ :=
    execution_requires_matching_review completeReview write basisSupplied basisMatches accepted
  exact FrKernels.Workflow.generated_lifecycle_finishes_applied exerciseReversal deliverPatch

end FrKernels.TaskChange
