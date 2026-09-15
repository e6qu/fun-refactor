namespace FrKernels.AgentIntent

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

end FrKernels.AgentIntent
