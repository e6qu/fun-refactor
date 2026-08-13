# expect: fails
# title: A RawOrder handed straight to price, rejected by the checker
# misuse-of: typed_pipeline
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


def price(order: Order, unit_cents: int) -> Priced:
    return Priced(item=order.item, total_cents=order.quantity * unit_cents)


quoted = price(RawOrder(item="saddle", quantity_text="2"), 250)
