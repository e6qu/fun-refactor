# expect: passes
# title: An alias is only a name: a bare count still passes as Meters
type Meters = float


def cut_tubing(length: Meters) -> str:
    return f"cutting {length}m of tubing"


def restock() -> str:
    spokes = 36
    return cut_tubing(spokes)
