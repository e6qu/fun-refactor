# expect: passes
# title: The function reads the clock, so every call answers differently
"""A test cannot pin this function's answer down. The answer depends on when
the test runs."""

import time


def remaining(deadline: float) -> float:
    return deadline - time.time()
