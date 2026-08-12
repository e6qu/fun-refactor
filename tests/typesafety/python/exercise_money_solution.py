# expect: passes
# title: The type system separates the currencies
# improves: exercise_money_start
"""One class per currency, and `add` constrained to a single one of them.
Cents are integers, so the totals are exact."""

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
    return add(Usd(1999), Usd(500))
