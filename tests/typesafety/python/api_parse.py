# expect: passes
# title: pydantic and zod check the order at the door
# improves: api_dict
from pydantic import BaseModel


class Order(BaseModel):
    id: str
    quantity: int
    gift_note: str | None = None


def price_cents(order: Order) -> int:
    return order.quantity * 250  # no checks: the type says both


def handle(body: str) -> int:
    order = Order.model_validate_json(body)
    return price_cents(order)
