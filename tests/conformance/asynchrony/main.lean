-- Lean separates what acts from what computes in the type, so a function that prints
-- answers in `IO` and so does everything that calls one.
def load (name : String) (base : Int) : IO Int := do
  IO.println s!"fetch {name}"
  return base + 1

def total (a : Int) (b : Int) : IO Int := do
  let first ← load "a" a
  let second ← load "b" b
  return first + second

def main : IO Unit := do
  IO.println "start"
  let result ← total 10 20
  IO.println s!"total {result}"
  IO.println "done"
