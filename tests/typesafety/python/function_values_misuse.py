# expect: fails
# title: A key that returns text where the sort needs a number, rejected by the checker
# misuse-of: function_values
from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Order:
    id: str
    total_cents: int


def cheapest_first(orders: list[Order], key: Callable[[Order], int]) -> list[Order]:
    return sorted(orders, key=key)


def demo(orders: list[Order]) -> list[Order]:
    return cheapest_first(orders, lambda order: order.id)
