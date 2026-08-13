# expect: fails
# title: Kilometers where Kilograms belong, rejected by the checker
# misuse-of: exercise_shipping_solution
from dataclasses import dataclass
from typing import NewType

Kilograms = NewType("Kilograms", float)
Kilometers = NewType("Kilometers", float)


@dataclass(frozen=True)
class Handling:
    express: bool = False
    insured: bool = False
    fragile: bool = False


def shipping_cents(weight: Kilograms, distance: Kilometers, handling: Handling) -> int:
    rate = 3 if handling.express else 1
    surcharge = (25 if handling.insured else 0) + (40 if handling.fragile else 0)
    return int(weight * distance * rate) + surcharge


quote = shipping_cents(
    Kilometers(120.0), Kilograms(2.5), Handling(express=True)  # rejected: both units are wrong
)
