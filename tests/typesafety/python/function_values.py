# expect: passes
# title: The key's full type catches the wrong lambda
# improves: function_any
"""`key` is a function from Order to int, and `Callable` writes that down.
The compiler checks the lambda against it."""

from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Order:
    id: str
    total_cents: int


def cheapest_first(orders: list[Order], key: Callable[[Order], int]) -> list[Order]:
    return sorted(orders, key=key)


def demo(orders: list[Order]) -> list[Order]:
    return cheapest_first(orders, lambda order: order.total_cents)
