structure LayoutRecord where
  count : Nat

def layoutRecord : Option LayoutRecord :=
  some {
    count := 1
  }

def literalBraces : String := "{not an interpolation}"
def interpolatedBraces (n : Nat) : String := s!"value {n}"

theorem inlineConstructorCases (n : Nat) : n = n := by
  cases n with | zero => rfl | succ k => rfl

theorem nestedConstructorCases (state action : Nat) : state = state := by
  cases state with
  | zero => rfl
  | succ state =>
      cases state with
      | zero =>
          cases action with
          | zero => rfl
          | succ action => rfl
      | succ state => rfl

theorem alternativeInsideConstructor (flag : Bool) (h : True ∨ True) : True := by
  cases flag with
  | false =>
    rcases h with left | right
    · exact left
    · exact right
  | true => trivial

def afterProjectLayout : Nat := 42
