namespace FrKernels.Technology

-- fr:spec src/project/technologies.rs::technology_evidence_emitted @ 0bdab9737605699b1377f1e52cbedb64d4ea4f6faf7585b6c9cbd54895350271
def evidenceEmitted (total limit : Nat) : Nat :=
  min total (min limit 32)

-- fr:spec src/project/technologies.rs::technology_evidence_omitted @ f388832653d3c59d40b6813712a782a938ab5c3f908fa8a49b8ed37b1494be93
def evidenceOmitted (total limit : Nat) : Nat :=
  total - evidenceEmitted total limit

theorem evidenceEmitted_le_limit (total limit : Nat) :
    evidenceEmitted total limit ≤ limit := by
  unfold evidenceEmitted
  omega

theorem evidenceEmitted_le_total (total limit : Nat) :
    evidenceEmitted total limit ≤ total := by
  exact Nat.min_le_left total (min limit 32)

theorem evidencePartition (total limit : Nat) :
    evidenceEmitted total limit + evidenceOmitted total limit = total := by
  unfold evidenceOmitted
  have bounded := evidenceEmitted_le_total total limit
  omega

end FrKernels.Technology
