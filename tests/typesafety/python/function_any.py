# expect: passes
# title: A loosely typed key accepts the wrong lambda
from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Order:
    id: str
    total_cents: int


def cheapest_first(orders: list[Order], key: Callable[..., int]) -> list[Order]:
    return sorted(orders, key=key)


def demo(orders: list[Order]) -> list[Order]:
    # accepted, and every call fails: sorted passes the key one argument, not two
    return cheapest_first(orders, lambda order, currency: int(order[currency]))
