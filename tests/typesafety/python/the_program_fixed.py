# expect: passes
# run: yes
# title: The bill of materials and the invoice, with the types doing the work
from dataclasses import dataclass
from enum import StrEnum
from typing import Literal, NewType

Pence = NewType("Pence", int)
Rate = NewType("Rate", float)
CustomerId = NewType("CustomerId", str)
ProductId = NewType("ProductId", str)
type Status = Literal["draft", "sent", "paid"]


class Unit(StrEnum):
    EACH = "each"
    METERS = "meters"
    KILOGRAMS = "kilograms"


@dataclass(frozen=True)
class BomLine:
    part_no: str
    description: str
    qty: int
    unit: Unit
    cost: Pence


def invoice_line(description: str, price: Pence, quantity: int, taxed: bool) -> str:
    note = " +tax" if taxed else ""
    return f"{description} x{quantity} at {price}d{note}"


def invoice_total(prices: list[Pence]) -> Pence:
    return Pence(sum(prices))


def apply_discount(total: Pence, rate: Rate) -> Pence:
    return Pence(round(total * (1 - rate)))


def advance(status: Status) -> Status:
    match status:
        case "draft":
            return "sent"
        case "sent":
            return "paid"
        case "paid":
            return "paid"


def bill(customer: CustomerId, product: ProductId) -> str:
    return f"invoice {customer} for one {product}"


def parse_bom_line(row: str) -> BomLine | None:
    match row.split(","):
        case [part_no, description, qty_text, unit_text, cost_text]:
            if not qty_text.isdigit() or not cost_text.isdigit():
                return None
            try:
                unit = Unit(unit_text)
            except ValueError:
                return None
            return BomLine(part_no, description, int(qty_text), unit, Pence(int(cost_text)))
        case _:
            return None


assert advance("draft") == "sent"
assert invoice_total([Pence(24), Pence(24), Pence(24)]) == 72
assert apply_discount(Pence(1250), Rate(0.1)) == 1125
assert parse_bom_line("F-101,down tube,1,meters,155") == BomLine(
    "F-101", "down tube", 1, Unit.METERS, Pence(155)
)
assert parse_bom_line("F-101,down tube,one,meters,155") is None
