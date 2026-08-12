# expect: passes
# title: Each step re-checks the dict it was handed
"""The steps pass a dict along. Each one checks the keys it needs, and a step
out of order fails at run time, when it fails at all."""


def parse_order(raw: dict[str, object]) -> dict[str, object]:
    quantity_text = raw.get("quantity_text")
    if not isinstance(quantity_text, str):
        raise ValueError("quantity_text missing")
    return {"item": raw.get("item"), "quantity": int(quantity_text)}


def price(order: dict[str, object], unit_cents: int) -> dict[str, object]:
    quantity = order.get("quantity")
    if not isinstance(quantity, int):
        raise ValueError("quantity missing")
    return {"item": order.get("item"), "total_cents": quantity * unit_cents}


def quote(raw: dict[str, object]) -> dict[str, object]:
    return price(parse_order(raw), unit_cents=250)
