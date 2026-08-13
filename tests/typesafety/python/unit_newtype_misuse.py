# expect: fails
# title: A count of spokes where Meters belong, rejected by the checker
# misuse-of: unit_newtype
from typing import NewType

Meters = NewType("Meters", float)
Each = NewType("Each", int)


def cut_tubing(length: Meters) -> str:
    return f"cutting {length}m of tubing"


def restock() -> str:
    spokes = Each(36)
    cut_tubing(1.8)
    return cut_tubing(spokes)
