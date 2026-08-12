# expect: passes
# title: Exercise 2, one solution
# improves: exercise_shipping_start
"""Units as types, options as named fields. The call now says what every
value means, and swapped units are a compile error."""

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
    Kilograms(2.5), Kilometers(120.0), Handling(express=True, fragile=True)
)
