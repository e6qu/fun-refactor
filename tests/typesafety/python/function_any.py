# expect: passes
# title: A loosely typed key accepts the wrong lambda
"""`Callable[..., int]` accepts any arguments at all, so a key function that
takes the wrong thing still compiles, and fails at run time instead."""

from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Order:
    id: str
    total_cents: int


def cheapest_first(orders: list[Order], key: Callable[..., int]) -> list[Order]:
    return sorted(orders, key=key)


def demo(orders: list[Order]) -> list[Order]:
    # The checker accepts this key, and every call of it fails at run time:
    # an Order has no `["total_cents"]`.
    return cheapest_first(orders, lambda order, currency: int(order[currency]))
