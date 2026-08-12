# expect: passes
# run: yes
# title: The clock arrives as a parameter, so equal inputs give equal answers
# improves: impure_clock
"""The function is now a table of facts. A test picks the moment and checks
the answer, today and every day after."""


def remaining(deadline: float, now: float) -> float:
    return deadline - now


assert remaining(deadline=120.0, now=45.0) == 75.0
assert remaining(deadline=120.0, now=45.0) == 75.0
