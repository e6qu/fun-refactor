# expect: passes
# title: Meters plus Kilograms still passes the check, and the sum is a meaningless number
from typing import NewType

Meters = NewType("Meters", float)
Kilograms = NewType("Kilograms", float)


def nonsense(tubing: Meters, grease: Kilograms) -> float:
    return tubing + grease
