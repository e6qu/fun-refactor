def main : IO Unit := do
  let word : String := "Hello"
  IO.println s!"upper {word.toUpper}"
  IO.println s!"lower {word.toLower}"
  IO.println s!"len {word.length}"
  IO.println s!"concat {word ++ "-" ++ "World"}"
  -- Lean has no `contains` on a string, and splitting on the needle finds it.
  if (word.splitOn "ell").length > 1 then
    IO.println "has yes"
  else
    IO.println "has no"
  if (word.splitOn "xyz").length > 1 then
    IO.println "has yes"
  else
    IO.println "has no"
  IO.println "done"
