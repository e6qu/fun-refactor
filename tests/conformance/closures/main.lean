def applyTo (f : Int → Int) (n : Int) : Int := f n

def main : IO Unit := do
  IO.println "start"
  let add3 : Int → Int := fun n => n + 3
  IO.println s!"apply {applyTo add3 4}"
  let double : Int → Int := fun n => n * 2
  IO.println s!"twice {applyTo double 6}"
  IO.println "done"
