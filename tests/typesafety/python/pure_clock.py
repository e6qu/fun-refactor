# expect: passes
# run: yes
# title: The clock as a parameter
"""`remaining_impure` reads the clock itself, so its answer changes between
calls and a test cannot pin it down. `remaining` takes the moment as an
argument: same input, same output, every time."""

import time


def remaining_impure(deadline: float) -> float:
    return deadline - time.time()  # a different answer every call


def remaining(deadline: float, now: float) -> float:
    return deadline - now


assert remaining(deadline=120.0, now=45.0) == 75.0
assert remaining(deadline=120.0, now=45.0) == 75.0  # and the same tomorrow
