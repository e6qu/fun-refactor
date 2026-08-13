# expect: fails
# title: The swapped discount call, rejected by the checker
# misuse-of: money_pence
from typing import NewType

Pence = NewType("Pence", int)
Rate = NewType("Rate", float)


def apply_discount(total: Pence, rate: Rate) -> Pence:
    return Pence(round(total * (1 - rate)))


def checkout() -> Pence:
    return apply_discount(Rate(0.1), Pence(1250))
