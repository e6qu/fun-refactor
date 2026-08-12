# expect: fails
# title: The wrong unit is now a type error
"""The same call as before, with a plain `int` and with `Meters`. Both are rejected."""

from typing import NewType

Seconds = NewType("Seconds", int)
Meters = NewType("Meters", int)


def wait_before_retry(delay: Seconds) -> str:
    return f"sleeping {delay}s"


def plan() -> str:
    distance = Meters(30)
    wait_before_retry(30)  # error: "int" is not "Seconds"
    return wait_before_retry(distance)  # error: "Meters" is not "Seconds"
