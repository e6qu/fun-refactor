# expect: passes
# title: NewType makes the wrong unit a type error
# improves: alias_transparent
"""`NewType` makes a distinct type from `int`. The checker tells them apart."""

from typing import NewType

Seconds = NewType("Seconds", int)
Meters = NewType("Meters", int)


def wait_before_retry(delay: Seconds) -> str:
    return f"sleeping {delay}s"


def plan() -> str:
    timeout = Seconds(30)
    return wait_before_retry(timeout)
