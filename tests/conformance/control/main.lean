def classify (n : Int) : String := Id.run do
  if n < 0 then
    return "negative"
  else
    if n == 0 then
      return "zero"
    else
      if n < 10 then
        return "small"
  return "large"

def main : IO Unit := do
  IO.println s!"classify -5 {classify (-5)}"
  IO.println s!"classify 0 {classify 0}"
  IO.println s!"classify 7 {classify 7}"
  IO.println s!"classify 40 {classify 40}"
  let mut i : Int := 0
  while (i < 6) do
    i := i + 1
    if Int.tmod i 2 == 0 then
      continue
    if i == 5 then
      break
    IO.println s!"odd {i}"
  for value in #[3, 1, 2] do
    IO.println s!"item {value}"
  for outer in [0:3] do
    for inner in [0:2] do
      IO.println s!"pair {outer} {inner}"
  IO.println "done"
