namespace FrKernels.DisclosedEdit

-- fr:spec src/project/author.rs::disclosed_edit_admitted @ 15409201d17aa87d3f500466cad12f0be0efbb5d4c759f1fc35512ef1f2d0489
-- fr:signature full_handle: bool => fullHandle: Bool; reference_format: bool => referenceFormat: Bool; candidate_count: usize => candidateCount: Nat; current_matches: bool => currentMatches: Bool; different: bool => different: Bool; return: bool => return: Bool
def admitted
    (fullHandle : Bool)
    (referenceFormat : Bool)
    (candidateCount : Nat)
    (currentMatches : Bool)
    (different : Bool) : Bool :=
  fullHandle && referenceFormat && candidateCount == 1 && currentMatches && different

theorem admitted_iff :
    admitted fullHandle referenceFormat candidateCount currentMatches different = true ↔
      fullHandle = true ∧ referenceFormat = true ∧ candidateCount = 1 ∧
        currentMatches = true ∧ different = true := by
  simp [admitted, and_assoc]

theorem admitted_selects_exactly_one
    (accepted : admitted fullHandle referenceFormat candidateCount currentMatches different = true) :
    candidateCount = 1 := by
  exact (admitted_iff.mp accepted).2.2.1

theorem stale_current_is_refused :
    admitted fullHandle referenceFormat candidateCount false different = false := by
  simp [admitted]

theorem unchanged_is_refused :
    admitted fullHandle referenceFormat candidateCount currentMatches false = false := by
  simp [admitted]

theorem malformed_capability_is_refused :
    admitted fullHandle false candidateCount currentMatches different = false := by
  simp [admitted]

end FrKernels.DisclosedEdit
