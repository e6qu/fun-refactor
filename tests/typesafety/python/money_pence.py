# expect: passes
# run: yes
# title: Pence and Rate become types, and money stops looking like every other number
# improves: money_float
from typing import NewType

Pence = NewType("Pence", int)
Rate = NewType("Rate", float)


def apply_discount(total: Pence, rate: Rate) -> Pence:
    return Pence(round(total * (1 - rate)))


assert apply_discount(Pence(1250), Rate(0.1)) == 1125
