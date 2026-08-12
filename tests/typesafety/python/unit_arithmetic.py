# expect: passes
# title: Arithmetic that keeps its units
"""`NewType` stops the wrong substitution, and arithmetic escapes it: adding two
of them is int + int again. A small class with a typed `__add__` keeps the unit
through the sum."""

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


def total_wait(first: Seconds, second: Seconds) -> Seconds:
    return first + second


def total_load(first: Kilograms, second: Kilograms) -> Kilograms:
    return first + second
