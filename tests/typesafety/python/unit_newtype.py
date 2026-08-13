# expect: passes
# title: Meters and Each become types the checker can tell apart
# improves: alias_transparent
from typing import NewType

Meters = NewType("Meters", float)
Each = NewType("Each", int)


def cut_tubing(length: Meters) -> str:
    return f"cutting {length}m of tubing"


def restock() -> str:
    frame_tubing = Meters(1.8)
    return cut_tubing(frame_tubing)
