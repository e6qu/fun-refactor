def main : IO Unit := do
  IO.println "start"
  let nums : Array Int := #[1, 2, 3, 4]
  let doubled := nums.map (fun n => n * 2)
  IO.println s!"first {doubled[0]!}"
  let mut total : Int := 0
  for d in doubled do
    total := total + d
  IO.println s!"total {total}"
  let big := nums.filter (fun n => n > 2)
  let mut kept : Int := 0
  for b in big do
    kept := kept + b
  IO.println s!"kept {kept}"
  IO.println "done"
