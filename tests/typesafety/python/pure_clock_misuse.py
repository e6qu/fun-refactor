# expect: fails
# title: A bare number where remaining needs a datetime, rejected by the checker
# misuse-of: pure_clock
from datetime import datetime, timedelta


def remaining(deadline: datetime, now: datetime) -> timedelta:
    return deadline - now


DISPATCH = datetime(2026, 8, 13, 16, 0)

left = remaining(DISPATCH, now=16.75)
