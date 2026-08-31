-- `Array` and not `List`: this indexes and grows, and a Lean list does neither in
-- constant time.
def main : IO Unit := do
  let mut nums : Array Int := #[]
  nums := nums.push 3
  nums := nums.push 1
  nums := nums.push 2
  IO.println s!"len {nums.size}"
  IO.println s!"first {nums[0]!}"
  nums := nums.set! 1 10
  let mut total : Int := 0
  for value in nums do
    total := total + value
  IO.println s!"sum {total}"
  for value in nums do
    IO.println s!"item {value}"
  IO.println "done"
