# expect: passes
# title: Typed arithmetic keeps the units: lengths add, and width times height is an area
# improves: newtype_arithmetic
from dataclasses import dataclass


@dataclass(frozen=True)
class Meters:
    value: float

    def __add__(self, other: "Meters") -> "Meters":
        return Meters(self.value + other.value)

    def __mul__(self, other: "Meters") -> "SquareMeters":
        return SquareMeters(self.value * other.value)


@dataclass(frozen=True)
class SquareMeters:
    value: float


@dataclass(frozen=True)
class Kilograms:
    value: float

    def __add__(self, other: "Kilograms") -> "Kilograms":
        return Kilograms(self.value + other.value)


def total_tubing(top_tube: Meters, down_tube: Meters) -> Meters:
    return top_tube + down_tube


def chain_guard_sheet(width: Meters, height: Meters) -> SquareMeters:
    return width * height


def shipping_weight(frame: Kilograms, wheels: Kilograms) -> Kilograms:
    return frame + wheels
