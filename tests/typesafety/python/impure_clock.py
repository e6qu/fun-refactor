# expect: passes
# title: remaining reads the clock, so its answer changes every second
"""A test cannot pin this function's answer down. The answer depends on when
the test runs."""

import time


def remaining(deadline: float) -> float:
    return deadline - time.time()
