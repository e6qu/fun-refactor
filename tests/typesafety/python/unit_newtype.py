# expect: passes
# title: Seconds and Meters become types the checker can tell apart
# improves: alias_transparent
from typing import NewType

Seconds = NewType("Seconds", int)
Meters = NewType("Meters", int)


def wait_before_retry(delay: Seconds) -> str:
    return f"sleeping {delay}s"


def plan() -> str:
    timeout = Seconds(30)
    return wait_before_retry(timeout)
