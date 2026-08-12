# expect: passes
# title: pydantic and zod check the order at the door
# improves: api_dict
"""The request body is a string. `model_validate_json` turns it into an `Order`
or raises, once, here. Every function past this point takes an `Order`."""

from pydantic import BaseModel


class Order(BaseModel):
    id: str
    quantity: int
    gift_note: str | None = None


def price_cents(order: Order) -> int:
    # No check that `quantity` exists, and no check that it is a number.
    # The type already says both.
    return order.quantity * 250


def handle(body: str) -> int:
    order = Order.model_validate_json(body)
    return price_cents(order)
