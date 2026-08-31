import FrKernels.Edit

open FrKernels

structure Case where
  name : String
  source : String
  edits : List Edit
  deriving Repr

def sources := ["", "a", "ab", "abc", "abcd"]

def replacements := ["", "X", "YZ"]

def editsFor (source : String) : List Edit :=
  (List.range (source.length + 1)).flatMap fun start =>
    (List.range (source.length - start + 1)).flatMap fun width =>
      replacements.map fun replacement => { start, stop := start + width, replacement }

def plansFor (source : String) : List (List Edit) :=
  let edits := editsFor source
  ([] :: edits.map (fun edit => [edit])) ++
    (edits.flatMap fun first => edits.map fun second => [first, second]) ++
      [[{ start := source.length, stop := source.length + 1, replacement := "X" }]]

def main : IO Unit :=
  for source in sources do
    for edits in plansFor source do
      match applyChecked source edits with
      | some output => IO.println s!"ok\t{output}"
      | none => IO.println "reject"
