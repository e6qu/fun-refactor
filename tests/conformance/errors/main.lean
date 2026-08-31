-- Lean's `panic!` answers with the type's default value and carries on, so a failure a
-- caller means to catch leaves through `IO`.
def check (n : Int) : IO Int := do
  if n < 0 then
    throw (IO.userError "negative")
  return n * 2

def double (n : Int) : IO Int := do
  let checked ← check n
  return checked + 1

def main : IO Unit := do
  try
    let v ← check 5
    IO.println s!"checked 5 -> {v}"
  catch e =>
    IO.println s!"caught {e}"
  try
    let v ← check (-3)
    IO.println s!"never {v}"
  catch e =>
    IO.println s!"caught {e}"
  try
    let v ← double 4
    IO.println s!"double 4 -> {v}"
  catch e =>
    IO.println s!"caught {e}"
  try
    let v ← double (-2)
    IO.println s!"never {v}"
  catch e =>
    IO.println s!"caught {e}"
  IO.println "done"
