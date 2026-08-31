import Std

def main : IO Unit := do
  IO.println "start"
  let mut seen : Std.HashSet String := Std.HashSet.emptyWithCapacity
  seen := seen.insert "ada"
  seen := seen.insert "alan"
  seen := seen.insert "ada"
  IO.println s!"size {seen.size}"
  if seen.contains "ada" then
    IO.println "has-ada yes"
  else
    IO.println "has-ada no"
  if seen.contains "grace" then
    IO.println "has-grace yes"
  else
    IO.println "has-grace no"
  seen := seen.erase "alan"
  IO.println s!"after {seen.size}"
  IO.println "done"
