-- A chain and not a `match`: Lean matches a value against the constructors of its
-- type, and these literals select on equality.
def dayName (day : Int) : String := Id.run do
  if day == 1 then
    return "mon"
  else
    if day == 2 then
      return "tue"
    else
      if day == 3 then
        return "wed"
      else
        return "other"

def opKind (word : String) : String := Id.run do
  if word == "add" then
    return "plus"
  else
    if word == "sub" then
      return "minus"
    else
      return "other"

def main : IO Unit := do
  IO.println s!"day 1 {dayName 1}"
  IO.println s!"day 3 {dayName 3}"
  IO.println s!"day 9 {dayName 9}"
  IO.println s!"kind add {opKind "add"}"
  IO.println s!"kind sub {opKind "sub"}"
  IO.println s!"kind mul {opKind "mul"}"
  IO.println "done"
