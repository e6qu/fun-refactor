# expect: passes
# title: remaining reads the clock, so its answer changes every second
from datetime import datetime, timedelta


def remaining(deadline: datetime) -> timedelta:
    return deadline - datetime.now()
