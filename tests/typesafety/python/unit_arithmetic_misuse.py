# expect: fails
# title: Adding seconds to kilograms fails to compile
# misuse-of: unit_arithmetic
"""The sum of two different units means nothing, and now the checker says so."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Seconds:
    value: float

    def __add__(self, other: "Seconds") -> "Seconds":
        return Seconds(self.value + other.value)


@dataclass(frozen=True)
class Kilograms:
    value: float

    def __add__(self, other: "Kilograms") -> "Kilograms":
        return Kilograms(self.value + other.value)


def nonsense(duration: Seconds, load: Kilograms) -> Seconds:
    return duration + load  # error: Seconds + Kilograms has no meaning
