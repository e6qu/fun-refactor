import Std

def main : IO Unit := do
  IO.println "start"
  let mut ages : Std.HashMap String Int := Std.HashMap.ofList [("ada", 36), ("alan", 41)]
  ages := ages.insert "grace" 45
  IO.println s!"size {ages.size}"
  IO.println s!"ada {ages.get! "ada"}"
  let mut total : Int := 0
  for name in #["ada", "alan", "grace"] do
    total := total + ages.get! name
  IO.println s!"total {total}"
  IO.println "done"
