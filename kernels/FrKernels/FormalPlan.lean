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

-- fr:spec src/spec.rs::proof_submission_admitted @ e7fcbafdd5fe75f2aaeff0494d06d3a4f2fdef615571528dadf442b7f77de890
-- fr:signature tactics_only: bool => tacticsOnly: Bool; nonempty: bool => nonempty: Bool; within_limit: bool => withinLimit: Bool; no_placeholders: bool => noPlaceholders: Bool; unique_region: bool => uniqueRegion: Bool; syntax_valid: bool => syntaxValid: Bool; lean_passed: bool => leanPassed: Bool; return: bool => return: Bool
def proofSubmissionAdmitted
    (tacticsOnly : Bool)
    (nonempty : Bool)
    (withinLimit : Bool)
    (noPlaceholders : Bool)
    (uniqueRegion : Bool)
    (syntaxValid : Bool)
    (leanPassed : Bool) : Bool :=
  tacticsOnly && nonempty && withinLimit && noPlaceholders && uniqueRegion && syntaxValid && leanPassed

theorem proof_submission_admitted_iff
    (tacticsOnly nonempty withinLimit noPlaceholders uniqueRegion syntaxValid leanPassed : Bool) :
    proofSubmissionAdmitted tacticsOnly nonempty withinLimit noPlaceholders uniqueRegion syntaxValid leanPassed = true ↔
      tacticsOnly = true ∧ nonempty = true ∧ withinLimit = true ∧ noPlaceholders = true ∧
        uniqueRegion = true ∧ syntaxValid = true ∧ leanPassed = true := by
  simp [proofSubmissionAdmitted, and_assoc]

-- fr:spec src/spec.rs::agent_property_admitted @ 211c5491cff12f3138cc531b39a98ebce5864d9f38bc19008cc9575cc45ae0d5
-- fr:signature schema_matches: bool => schemaMatches: Bool; task_matches: bool => taskMatches: Bool; safe_names: bool => safeNames: Bool; types_disclosed: bool => typesDisclosed: Bool; within_limits: bool => withinLimits: Bool; terms_well_typed: bool => termsWellTyped: Bool; proposition_well_typed: bool => propositionWellTyped: Bool; return: bool => return: Bool
def agentPropertyAdmitted
    (schemaMatches : Bool)
    (taskMatches : Bool)
    (safeNames : Bool)
    (typesDisclosed : Bool)
    (withinLimits : Bool)
    (termsWellTyped : Bool)
    (propositionWellTyped : Bool) : Bool :=
  schemaMatches && taskMatches && safeNames && typesDisclosed && withinLimits && termsWellTyped &&
    propositionWellTyped

theorem agent_property_admitted_iff
    (schemaMatches taskMatches safeNames typesDisclosed withinLimits termsWellTyped
      propositionWellTyped : Bool) :
    agentPropertyAdmitted schemaMatches taskMatches safeNames typesDisclosed withinLimits
      termsWellTyped propositionWellTyped = true ↔
    schemaMatches = true ∧ taskMatches = true ∧ safeNames = true ∧ typesDisclosed = true ∧
      withinLimits = true ∧ termsWellTyped = true ∧ propositionWellTyped = true := by
  simp [agentPropertyAdmitted, and_assoc]

-- fr:spec src/spec.rs::agent_term_operator_admitted @ 666b4d5626b09c469498f2b35e419164adf32edb2f1c8ded49097fbf31917cd2
-- fr:signature operator: u8 => operator: Nat; operand_type: u8 => operandType: Nat; return: bool => return: Bool
def agentTermOperatorAdmitted (operator : Nat) (operandType : Nat) : Bool :=
  if operator == 0 then operandType == 0
  else if operator == 1 then operandType == 2
  else if operator == 2 || operator == 3 || operator == 4 then
    operandType == 1 || operandType == 2
  else if operator == 5 || operator == 6 then operandType == 0
  else false

theorem agent_term_not_requires_bool : agentTermOperatorAdmitted 0 0 = true := by
  rfl

theorem agent_term_negate_requires_int : agentTermOperatorAdmitted 1 2 = true := by
  rfl

theorem agent_term_unknown_operator_refuses (operandType : Nat) :
    agentTermOperatorAdmitted 7 operandType = false := by
  simp [agentTermOperatorAdmitted]

-- fr:spec src/spec.rs::agent_relation_admitted @ 7c3590f6ed76ada7f2c0d87932093e9d76ca054b671dd218fed77cae01684a80
-- fr:signature relation: u8 => relation: Nat; left_type: u8 => leftType: Nat; right_type: u8 => rightType: Nat; return: bool => return: Bool
def agentRelationAdmitted (relation : Nat) (leftType : Nat) (rightType : Nat) : Bool :=
  if relation == 0 || relation == 1 then leftType < 4 && leftType == rightType
  else if relation == 2 || relation == 3 || relation == 4 || relation == 5 then
    (leftType == 1 || leftType == 2) && leftType == rightType
  else if relation == 6 then leftType == 0
  else false

theorem agent_relation_equality_requires_matching_types (leftType rightType : Nat) :
    agentRelationAdmitted 0 leftType rightType = true ↔ leftType < 4 ∧ leftType = rightType := by
  simp [agentRelationAdmitted]

theorem agent_relation_holds_requires_bool (leftType rightType : Nat) :
    agentRelationAdmitted 6 leftType rightType = true ↔ leftType = 0 := by
  simp [agentRelationAdmitted]

end FrKernels.FormalPlan
