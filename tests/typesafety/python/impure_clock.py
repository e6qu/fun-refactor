# expect: passes
# title: remaining reads the clock, so its answer changes every second
import time


def remaining(deadline: float) -> float:
    return deadline - time.time()
