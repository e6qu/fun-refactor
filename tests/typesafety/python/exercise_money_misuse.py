# expect: fails
# title: Adding dollars to euros fails to compile
# misuse-of: exercise_money_solution
"""The mixed-currency call from the start, against the typed version."""

from dataclasses import dataclass, replace


@dataclass(frozen=True)
class Usd:
    cents: int


@dataclass(frozen=True)
class Eur:
    cents: int


def add[M: (Usd, Eur)](a: M, b: M) -> M:
    return replace(a, cents=a.cents + b.cents)


def basket_total() -> Usd:
    return add(Usd(1999), Eur(500))  # error: no single currency fits both
