# expect: passes
# title: The sum of two NewType numbers comes back as a plain int
"""`NewType` guards substitution and arithmetic walks around it. Both units
are ints underneath, so the checker accepts the sum and the unit is gone."""

from typing import NewType

Seconds = NewType("Seconds", int)
Kilograms = NewType("Kilograms", int)


def nonsense(duration: Seconds, load: Kilograms) -> int:
    # The checker accepts this line. The result means nothing.
    return duration + load
