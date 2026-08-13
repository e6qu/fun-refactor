# expect: passes
# title: The bill of materials and the invoice, as they stand today
from typing import Any


def bom_line(part_no: Any, description: Any, qty: Any, unit: Any, cost: Any) -> Any:
    return f"{part_no}  {description}  {qty} {unit} at {cost}d"


def product_cost(costs_pounds: list[float]) -> float:
    return sum(costs_pounds)


def invoice_line(description: Any, price_pence: Any, quantity: Any, taxed: Any) -> Any:
    note = " +tax" if taxed else ""
    return f"{description} x{quantity} at {price_pence}d{note}"


def invoice_total(prices_pounds: list[float]) -> float:
    return sum(prices_pounds)


def apply_discount(total_pounds: float, rate: float) -> float:
    return total_pounds * (1 - rate)


def advance(status: str) -> str:
    if status == "darft":
        return "sent"
    if status == "sent":
        return "paid"
    return status


def bill(customer_id: str, product_id: str) -> str:
    return f"invoice {customer_id} for one {product_id}"


def load_bom_line(row: str) -> list[str]:
    return row.split(",")
