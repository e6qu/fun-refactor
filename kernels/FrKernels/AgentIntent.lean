namespace FrKernels.AgentIntent

-- fr:spec src/project/agent_intent.rs::agent_intent_section_allowed @ 9cf9e503721b3830c3f2ac8b66548947f9714ccb83b2596c4c3438a2cbc9c5ad
-- fr:signature purpose: usize => purpose: Nat; section_code: usize => sectionCode: Nat; return: bool => return: Bool
def sectionAllowed (purpose : Nat) (sectionCode : Nat) : Bool :=
  match purpose with
  | 0 => sectionCode == 0
  | 1 => sectionCode == 0 || sectionCode == 1 || sectionCode == 3
  | 2 => sectionCode == 0 || sectionCode == 2
  | 3 => sectionCode == 0 || sectionCode == 2 || sectionCode == 3
  | 4 => sectionCode == 0 || sectionCode == 2
  | _ => false

theorem understand_admits_exactly_code_map (sectionCode : Nat) :
    sectionAllowed 0 sectionCode = true ↔ sectionCode = 0 := by
  simp [sectionAllowed]

theorem trace_admits_exactly_declared_sections (sectionCode : Nat) :
    sectionAllowed 1 sectionCode = true ↔
      (sectionCode = 0 ∨ sectionCode = 1) ∨ sectionCode = 3 := by
  simp [sectionAllowed]

theorem change_admits_exactly_declared_sections (sectionCode : Nat) :
    sectionAllowed 2 sectionCode = true ↔ sectionCode = 0 ∨ sectionCode = 2 := by
  simp [sectionAllowed]

theorem migrate_admits_exactly_declared_sections (sectionCode : Nat) :
    sectionAllowed 3 sectionCode = true ↔
      (sectionCode = 0 ∨ sectionCode = 2) ∨ sectionCode = 3 := by
  simp [sectionAllowed]

theorem prove_admits_exactly_declared_sections (sectionCode : Nat) :
    sectionAllowed 4 sectionCode = true ↔ sectionCode = 0 ∨ sectionCode = 2 := by
  simp [sectionAllowed]

def boundsAdmitted
    (needs sections calls callLimit packetBytes packetLimit : Nat) : Bool :=
  1 ≤ needs && needs ≤ 32 && 1 ≤ sections && sections ≤ 8 && sections ≤ needs &&
    1 ≤ callLimit && callLimit ≤ 512 && calls ≤ callLimit &&
    1024 ≤ packetLimit && packetLimit ≤ 65536 && packetBytes ≤ packetLimit

-- fr:spec src/project/disclose.rs::agent_intent_admitted @ 5f7a715a0adfb9d1a5ecb236a50e3edf71f403b030ba5c9bc5d285f3c7c28e0f
-- fr:signature needs: usize => needs: Nat; sections: usize => sections: Nat; calls: usize => calls: Nat; call_limit: usize => callLimit: Nat; packet_bytes: usize => packetBytes: Nat; packet_limit: usize => packetLimit: Nat; target_matches: bool => targetMatches: Bool; session_matches: bool => sessionMatches: Bool; complete: bool => complete: Bool; return: bool => return: Bool
def admitted
    (needs : Nat)
    (sections : Nat)
    (calls : Nat)
    (callLimit : Nat)
    (packetBytes : Nat)
    (packetLimit : Nat)
    (targetMatches : Bool)
    (sessionMatches : Bool)
    (complete : Bool) : Bool :=
  boundsAdmitted needs sections calls callLimit packetBytes packetLimit &&
    targetMatches && sessionMatches && complete

theorem admitted_intent_is_bounded
    (needs sections calls callLimit packetBytes packetLimit : Nat)
    (targetMatches sessionMatches complete : Bool)
    (accepted : admitted needs sections calls callLimit packetBytes packetLimit
      targetMatches sessionMatches complete = true) :
    boundsAdmitted needs sections calls callLimit packetBytes packetLimit = true := by
  simp [admitted] at accepted
  exact accepted.1.1.1

theorem admitted_intent_matches_all_evidence
    (needs sections calls callLimit packetBytes packetLimit : Nat)
    (targetMatches sessionMatches complete : Bool)
    (accepted : admitted needs sections calls callLimit packetBytes packetLimit
      targetMatches sessionMatches complete = true) :
    targetMatches = true ∧ sessionMatches = true ∧ complete = true := by
  simp [admitted] at accepted
  exact ⟨accepted.1.1.2, accepted.1.2, accepted.2⟩

-- fr:spec src/project/agent_intent.rs::agent_action_mode @ f8c5adf9fedce1bb11b50eb60b690833549f6e91724cf59bb17d065ed612fc7c
-- fr:signature purpose: usize => purpose: Nat; action_complete: bool => actionComplete: Bool; write: bool => write: Bool; basis_supplied: bool => basisSupplied: Bool; basis_matches: bool => basisMatches: Bool; return: usize => return: Nat
def actionMode
    (purpose : Nat)
    (actionComplete : Bool)
    (write : Bool)
    (basisSupplied : Bool)
    (basisMatches : Bool) : Nat :=
  if purpose == 2 && actionComplete && !write && !basisSupplied then 0
  else if purpose == 2 && actionComplete && write && basisSupplied && basisMatches then 1
  else 2

theorem action_preview_is_exact
    (purpose : Nat)
    (actionComplete write basisSupplied basisMatches : Bool) :
    actionMode purpose actionComplete write basisSupplied basisMatches = 0 ↔
      purpose = 2 ∧ actionComplete = true ∧ write = false ∧ basisSupplied = false := by
  by_cases h : purpose = 2 <;>
    cases actionComplete <;> cases write <;> cases basisSupplied <;> cases basisMatches <;>
      simp_all [actionMode]

theorem action_execution_is_exact
    (purpose : Nat)
    (actionComplete write basisSupplied basisMatches : Bool) :
    actionMode purpose actionComplete write basisSupplied basisMatches = 1 ↔
      purpose = 2 ∧ actionComplete = true ∧ write = true ∧
        basisSupplied = true ∧ basisMatches = true := by
  by_cases h : purpose = 2 <;>
    cases actionComplete <;> cases write <;> cases basisSupplied <;> cases basisMatches <;>
      simp_all [actionMode]

end FrKernels.AgentIntent
