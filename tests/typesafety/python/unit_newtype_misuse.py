# expect: fails
# title: Meters where Seconds belong, rejected by the checker
# misuse-of: unit_newtype
from typing import NewType

Seconds = NewType("Seconds", int)
Meters = NewType("Meters", int)


def wait_before_retry(delay: Seconds) -> str:
    return f"sleeping {delay}s"


def plan() -> str:
    distance = Meters(30)
    wait_before_retry(30)  # error: "int" is not "Seconds"
    return wait_before_retry(distance)  # error: "Meters" is not "Seconds"
