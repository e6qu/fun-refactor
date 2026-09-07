namespace FrKernels.Git

-- fr:spec src/git.rs::line_in_range @ f00499f53d1e0b9ac75d0f75952d8b2c44538585812a0456462394a25aba1670
-- fr:signature start: usize => start: Nat; end: usize => finish: Nat; line: usize => line: Nat; return: bool => return: Bool
def lineInRange (start : Nat) (finish : Nat) (line : Nat) : Bool :=
  decide (start ≤ line ∧ line ≤ finish)

theorem line_in_range_iff (start finish line : Nat) :
    lineInRange start finish line = true ↔ start ≤ line ∧ line ≤ finish := by
  simp [lineInRange]

theorem before_range_refuses (start finish line : Nat) (h : line < start) :
    lineInRange start finish line = false := by
  simp [lineInRange]
  omega

theorem after_range_refuses (start finish line : Nat) (h : finish < line) :
    lineInRange start finish line = false := by
  simp [lineInRange]
  omega

theorem reversed_range_refuses (start finish line : Nat) (h : finish < start) :
    lineInRange start finish line = false := by
  simp [lineInRange]
  omega

theorem singleton_matches (point line : Nat) :
    lineInRange point point line = true ↔ line = point := by
  simp [lineInRange]
  omega

theorem enclosing_range_preserves_match (start finish outerStart outerFinish line : Nat)
    (left : outerStart ≤ start) (right : finish ≤ outerFinish)
    (inside : lineInRange start finish line = true) :
    lineInRange outerStart outerFinish line = true := by
  simp [lineInRange] at *
  omega

-- fr:spec src/git.rs::call_in_selection @ 2d90cfbdd483cfcbed3ee14e4e40f8ef3d794768e793d1fac571c521575c5eca
-- fr:signature incoming: bool => incoming: Bool; outgoing: bool => outgoing: Bool; include_incoming: bool => includeIncoming: Bool; include_outgoing: bool => includeOutgoing: Bool; return: bool => return: Bool
def callInSelection (incoming : Bool) (outgoing : Bool)
    (includeIncoming : Bool) (includeOutgoing : Bool) : Bool :=
  (incoming && includeIncoming) || (outgoing && includeOutgoing)

theorem no_directions_refuses (incoming outgoing : Bool) :
    callInSelection incoming outgoing false false = false := by
  cases incoming <;> cases outgoing <;> rfl

theorem no_endpoints_refuses (includeIncoming includeOutgoing : Bool) :
    callInSelection false false includeIncoming includeOutgoing = false := by
  rfl

theorem incoming_only (incoming outgoing : Bool) :
    callInSelection incoming outgoing true false = incoming := by
  cases incoming <;> cases outgoing <;> rfl

theorem outgoing_only (incoming outgoing : Bool) :
    callInSelection incoming outgoing false true = outgoing := by
  cases incoming <;> cases outgoing <;> rfl

theorem both_directions (incoming outgoing : Bool) :
    callInSelection incoming outgoing true true = (incoming || outgoing) := by
  cases incoming <;> cases outgoing <;> rfl

theorem swapping_sides_preserves_selection (incoming outgoing includeIncoming includeOutgoing : Bool) :
    callInSelection incoming outgoing includeIncoming includeOutgoing =
      callInSelection outgoing incoming includeOutgoing includeIncoming := by
  cases incoming <;> cases outgoing <;> cases includeIncoming <;> cases includeOutgoing <;> rfl

end FrKernels.Git
