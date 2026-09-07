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

end FrKernels.Git
