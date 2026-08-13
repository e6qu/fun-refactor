# expect: passes
# title: parse_order returns an Order, and price accepts only an Order
# improves: pipeline_dicts
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
    if not raw.quantity_text.isdigit():
        raise ValueError(f"quantity is not a number: {raw.quantity_text}")
    return Order(item=raw.item, quantity=int(raw.quantity_text))


def price(order: Order, unit_cents: int) -> Priced:
    return Priced(item=order.item, total_cents=order.quantity * unit_cents)


def quote(raw: RawOrder) -> Priced:
    return price(parse_order(raw), unit_cents=250)
