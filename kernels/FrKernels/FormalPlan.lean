namespace FrKernels.FormalPlan

-- fr:spec src/spec.rs::formal_candidate_admitted @ 2b6192b9d49327ea5f42382902dc27e31d619bbd9d9acea85c098e607c1bbcca
-- fr:signature source_is_rust: bool => sourceIsRust: Bool; top_level: bool => topLevel: Bool; typed: bool => typed: Bool; pure: bool => pure: Bool; body_supported: bool => bodySupported: Bool; return: bool => return: Bool
def candidateAdmitted
    (sourceIsRust : Bool)
    (topLevel : Bool)
    (typed : Bool)
    (pure : Bool)
    (bodySupported : Bool) : Bool :=
  sourceIsRust && topLevel && typed && pure && bodySupported

theorem candidate_admitted_iff (sourceIsRust topLevel typed pure bodySupported : Bool) :
    candidateAdmitted sourceIsRust topLevel typed pure bodySupported = true ↔
      sourceIsRust = true ∧ topLevel = true ∧ typed = true ∧ pure = true ∧ bodySupported = true := by
  simp [candidateAdmitted, and_assoc]

-- fr:spec src/spec.rs::formal_property_admitted @ 9006f0951da3b6c99f7ab603ac581dbed9a1526c64da758159f5e06b9fc1ffb7
-- fr:signature known_kind: bool => knownKind: Bool; one_input: bool => oneInput: Bool; input_matches_output: bool => inputMatchesOutput: Bool; boolean_surface: bool => booleanSurface: Bool; return: bool => return: Bool
def propertyAdmitted
    (knownKind : Bool)
    (oneInput : Bool)
    (inputMatchesOutput : Bool)
    (booleanSurface : Bool) : Bool :=
  knownKind && oneInput && (inputMatchesOutput || booleanSurface)

theorem property_rejects_unknown (oneInput inputMatchesOutput booleanSurface : Bool) :
    propertyAdmitted false oneInput inputMatchesOutput booleanSurface = false := by
  rfl

theorem property_requires_one_input (knownKind inputMatchesOutput booleanSurface : Bool) :
    propertyAdmitted knownKind false inputMatchesOutput booleanSurface = false := by
  simp [propertyAdmitted]

end FrKernels.FormalPlan
