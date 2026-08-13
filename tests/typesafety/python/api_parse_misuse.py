# expect: fails
# title: A bare dictionary where an Order is required, rejected by the checker
# misuse-of: api_parse
from pydantic import BaseModel, ConfigDict


class Order(BaseModel):
    model_config = ConfigDict(strict=True)

    id: str
    quantity: int
    gift_note: str | None


def price_cents(order: Order) -> int:
    return order.quantity * 250


total = price_cents({"id": "o1", "quantity": "2", "gift_note": None})
