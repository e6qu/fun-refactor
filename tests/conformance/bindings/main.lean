-- `Int` and not `Nat`: Lean reads a bare digit as a natural number, whose subtraction
-- stops at zero.
def main : IO Unit := do
  IO.println "start"
  let n : Int := 42
  let mut total : Int := n + 10
  IO.println s!"n {n}"
  IO.println s!"sum {total}"
  total := total * 2
  IO.println s!"twice {total}"
  -- `/` on `Int` rounds toward negative infinity and `%` is the Euclidean remainder,
  -- so the division that truncates says which one it wants.
  let q : Int := Int.tdiv 10 3
  let r : Int := Int.tmod 10 3
  IO.println s!"q {q} r {r}"
  let label : String := s!"item-{7}"
  IO.println s!"label {label}"
  let mut i : Int := 0
  while (i < 3) do
    IO.println s!"tick {i}"
    i := i + 1
  IO.println "done"
