# expect: passes
# title: Steps that only fit one way
"""Each step's output type is the next step's input type. The chain compiles
only when they meet, so a step out of order is a compile error."""

from dataclasses import dataclass


@dataclass(frozen=True)
class RawOrder:
    item: str
    quantity_text: str


@dataclass(frozen=True)
class Order:
    item: str
    quantity: int


@dataclass(frozen=True)
class Priced:
    item: str
    total_cents: int


def parse_order(raw: RawOrder) -> Order:
    return Order(item=raw.item, quantity=int(raw.quantity_text))


def price(order: Order, unit_cents: int) -> Priced:
    return Priced(item=order.item, total_cents=order.quantity * unit_cents)


def quote(raw: RawOrder) -> Priced:
    return price(parse_order(raw), unit_cents=250)
