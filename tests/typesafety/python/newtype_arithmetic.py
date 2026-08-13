# expect: passes
# title: Seconds plus Kilograms still passes the check, and the sum is a meaningless int
from typing import NewType

Seconds = NewType("Seconds", int)
Kilograms = NewType("Kilograms", int)


def nonsense(duration: Seconds, load: Kilograms) -> int:
    return duration + load  # accepted, and the sum means nothing
