namespace FrKernels.AgentContext

-- fr:spec src/project/disclose.rs::context_materialization_admitted @ 1bd1dac8915a92479584a19403b97e36d9e82564b2449b514efdfddd3f233070
-- fr:signature calls: usize => calls: Nat; call_limit: usize => callLimit: Nat; session_matches: bool => sessionMatches: Bool; complete: bool => complete: Bool; digest_matches: bool => digestMatches: Bool; return: bool => return: Bool
def materializationAdmitted
    (calls : Nat)
    (callLimit : Nat)
    (sessionMatches : Bool)
    (complete : Bool)
    (digestMatches : Bool) : Bool :=
  1 ≤ callLimit && callLimit ≤ 64 && calls ≤ callLimit &&
    sessionMatches && complete && digestMatches

theorem admitted_materialization_is_bounded
    (calls callLimit : Nat)
    (sessionMatches complete digestMatches : Bool)
    (accepted : materializationAdmitted calls callLimit sessionMatches complete digestMatches = true) :
    1 ≤ callLimit ∧ callLimit ≤ 64 ∧ calls ≤ callLimit := by
  simp [materializationAdmitted] at accepted
  exact ⟨accepted.1.1.1.1.1, accepted.1.1.1.1.2, accepted.1.1.1.2⟩

theorem admitted_materialization_matches_all_evidence
    (calls callLimit : Nat)
    (sessionMatches complete digestMatches : Bool)
    (accepted : materializationAdmitted calls callLimit sessionMatches complete digestMatches = true) :
    sessionMatches = true ∧ complete = true ∧ digestMatches = true := by
  simp [materializationAdmitted] at accepted
  exact ⟨accepted.1.1.2, accepted.1.2, accepted.2⟩

-- fr:spec src/project/disclose.rs::object_store_admitted @ 07f3c15f377964db3c2699411639475780457a49aea9c6e2fc6442ffe0938eca
-- fr:signature objects: usize => objects: Nat; encoded_bytes: usize => encodedBytes: Nat; digest_matches: bool => digestMatches: Bool; records_canonical: bool => recordsCanonical: Bool; root_present: bool => rootPresent: Bool; return: bool => return: Bool
def objectStoreAdmitted
    (objects : Nat)
    (encodedBytes : Nat)
    (digestMatches : Bool)
    (recordsCanonical : Bool)
    (rootPresent : Bool) : Bool :=
  1 ≤ objects && objects ≤ 65536 && encodedBytes ≤ 67108864 &&
    digestMatches && recordsCanonical && rootPresent

theorem admitted_store_is_bounded
    (objects encodedBytes : Nat)
    (digestMatches recordsCanonical rootPresent : Bool)
    (accepted : objectStoreAdmitted objects encodedBytes digestMatches recordsCanonical rootPresent = true) :
    1 ≤ objects ∧ objects ≤ 65536 ∧ encodedBytes ≤ 67108864 := by
  simp [objectStoreAdmitted] at accepted
  exact ⟨accepted.1.1.1.1.1, accepted.1.1.1.1.2, accepted.1.1.1.2⟩

theorem admitted_store_matches_all_evidence
    (objects encodedBytes : Nat)
    (digestMatches recordsCanonical rootPresent : Bool)
    (accepted : objectStoreAdmitted objects encodedBytes digestMatches recordsCanonical rootPresent = true) :
    digestMatches = true ∧ recordsCanonical = true ∧ rootPresent = true := by
  simp [objectStoreAdmitted] at accepted
  exact ⟨accepted.1.1.2, accepted.1.2, accepted.2⟩

end FrKernels.AgentContext
