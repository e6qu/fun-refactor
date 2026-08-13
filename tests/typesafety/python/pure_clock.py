# expect: passes
# run: yes
# title: remaining takes now as a parameter, so a test can pin its answer
# improves: impure_clock
from datetime import datetime, timedelta


def remaining(deadline: datetime, now: datetime) -> timedelta:
    return deadline - now


DISPATCH = datetime(2026, 8, 13, 16, 0)

assert remaining(DISPATCH, now=datetime(2026, 8, 13, 15, 30)) == timedelta(minutes=30)
assert remaining(DISPATCH, now=datetime(2026, 8, 13, 16, 45)) == timedelta(minutes=-45)
