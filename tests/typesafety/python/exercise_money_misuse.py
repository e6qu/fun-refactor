# expect: fails
# title: Usd plus Eur, rejected by the checker
# misuse-of: exercise_money_solution
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
    return add(Usd(1999), Eur(500))
