# expect: passes
# title: The handler digs through a dictionary and checks as it goes
"""The body is parsed into a dict, and the shape checks spread through every
function that reads it."""

import json


def price_cents(order: dict[str, object]) -> int:
    quantity = order.get("quantity")
    if not isinstance(quantity, int):
        raise ValueError("quantity missing or not a number")
    return quantity * 250


def handle(body: str) -> int:
    order = json.loads(body)
    if not isinstance(order, dict):
        raise ValueError("not an object")
    return price_cents(order)
