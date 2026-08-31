-- Lean's own `/` on `Int` already rounds toward negative infinity, which is what these
-- two functions compute the long way everywhere else.
def floorDiv (a : Int) (b : Int) : Int := Id.run do
  let quotient : Int := Int.tdiv a b
  if Int.tmod a b != 0 && (a < 0) != (b < 0) then
    return quotient - 1
  return quotient

def floorMod (a : Int) (b : Int) : Int := a - floorDiv a b * b

def main : IO Unit := do
  IO.println "start"
  let a : Int := 7
  let b : Int := 2
  IO.println s!"sum {a + b}"
  IO.println s!"diff {a - b}"
  IO.println s!"product {a * b}"
  IO.println s!"quotient {floorDiv a b}"
  IO.println s!"remainder {floorMod a b}"
  let negative : Int := -7
  IO.println s!"negquotient {floorDiv negative b}"
  IO.println s!"negremainder {floorMod negative b}"
  IO.println "done"
