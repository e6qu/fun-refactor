structure Box where
  value : Int
deriving Repr, Inhabited, BEq

def Box.get (self : Box) : Int := self.value

def firstOf (items : Array Int) : Int := items[0]!

def countOf (items : Array String) : Int := items.size

def main : IO Unit := do
  IO.println "start"
  let numbers : Array Int := #[4, 5, 6]
  let words : Array String := #["a", "b"]
  IO.println s!"first {firstOf numbers}"
  IO.println s!"count {countOf words}"
  let b : Box := { value := 9 }
  IO.println s!"box {b.get}"
  IO.println "done"
