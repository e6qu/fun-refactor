# expect: passes
# title: With typed addition, Seconds plus Seconds stays Seconds
# improves: newtype_arithmetic
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
