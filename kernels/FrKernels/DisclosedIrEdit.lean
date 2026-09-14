namespace FrKernels.DisclosedIrEdit

-- fr:spec src/project/author.rs::disclosed_ir_edit_admitted @ 1c506b5fa977e64f2df0dd2df6f3f21f7fe41bd62a88493a12f032c147954a57
-- fr:signature full_handle: bool => fullHandle: Bool; reference_format: bool => referenceFormat: Bool; candidate_count: usize => candidateCount: Nat; current_matches: bool => currentMatches: Bool; request_shape: bool => requestShape: Bool; value_matches: bool => valueMatches: Bool; different: bool => different: Bool; return: bool => return: Bool
def admitted
    (fullHandle : Bool)
    (referenceFormat : Bool)
    (candidateCount : Nat)
    (currentMatches : Bool)
    (requestShape : Bool)
    (valueMatches : Bool)
    (different : Bool) : Bool :=
  fullHandle && referenceFormat && candidateCount == 1 && currentMatches && requestShape &&
    valueMatches && different

theorem admitted_iff :
    admitted fullHandle referenceFormat candidateCount currentMatches requestShape valueMatches
        different = true ↔
      fullHandle = true ∧ referenceFormat = true ∧ candidateCount = 1 ∧
        currentMatches = true ∧ requestShape = true ∧ valueMatches = true ∧ different = true := by
  simp [admitted, and_assoc]

theorem admitted_selects_exactly_one
    (accepted : admitted fullHandle referenceFormat candidateCount currentMatches requestShape
      valueMatches different = true) : candidateCount = 1 := by
  exact (admitted_iff.mp accepted).2.2.1

theorem stale_current_is_refused :
    admitted fullHandle referenceFormat candidateCount false requestShape valueMatches different =
      false := by
  simp [admitted]

theorem malformed_request_is_refused :
    admitted fullHandle referenceFormat candidateCount currentMatches false valueMatches different =
      false := by
  simp [admitted]

theorem category_mismatch_is_refused :
    admitted fullHandle referenceFormat candidateCount currentMatches requestShape false different =
      false := by
  simp [admitted]

theorem unchanged_is_refused :
    admitted fullHandle referenceFormat candidateCount currentMatches requestShape valueMatches false =
      false := by
  simp [admitted]

theorem admitted_requires_current_typed_value
    (accepted : admitted fullHandle referenceFormat candidateCount currentMatches requestShape
      valueMatches different = true) :
    currentMatches = true ∧ requestShape = true ∧ valueMatches = true := by
  exact ⟨(admitted_iff.mp accepted).2.2.2.1,
    (admitted_iff.mp accepted).2.2.2.2.1,
    (admitted_iff.mp accepted).2.2.2.2.2.1⟩

end FrKernels.DisclosedIrEdit
