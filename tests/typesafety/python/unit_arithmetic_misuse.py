# expect: fails
# title: Meters plus Kilograms, rejected by the checker
# misuse-of: unit_arithmetic
from dataclasses import dataclass


@dataclass(frozen=True)
class Meters:
    value: float

    def __add__(self, other: "Meters") -> "Meters":
        return Meters(self.value + other.value)


@dataclass(frozen=True)
class Kilograms:
    value: float

    def __add__(self, other: "Kilograms") -> "Kilograms":
        return Kilograms(self.value + other.value)


def nonsense(tubing: Meters, grease: Kilograms) -> Meters:
    return tubing + grease
