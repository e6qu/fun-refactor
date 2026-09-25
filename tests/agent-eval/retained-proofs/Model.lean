namespace FrSpecs

def allowed (ok : Bool) : Bool := ok

theorem identity (ok : Bool) : allowed ok = ok := by rfl

end FrSpecs
