# expect: passes
# run: yes
# title: The bill of materials and the invoice, with the types doing the work
from dataclasses import dataclass
from typing import Literal, NewType, TypeIs

Pence = NewType("Pence", int)
CustomerId = NewType("CustomerId", str)
ProductId = NewType("ProductId", str)
type Status = Literal["draft", "sent", "paid"]
type Unit = Literal["each", "meters", "kilograms"]


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


def is_unit(value: str) -> TypeIs[Unit]:
    return value in ("each", "meters", "kilograms")


def parse_bom_line(row: str) -> BomLine | None:
    match row.split(","):
        case [part_no, description, qty_text, unit_text, cost_text]:
            if not qty_text.isdigit() or not cost_text.isdigit() or not is_unit(unit_text):
                return None
            return BomLine(part_no, description, int(qty_text), unit_text, Pence(int(cost_text)))
        case _:
            return None


assert advance("draft") == "sent"
assert invoice_total([Pence(24), Pence(24), Pence(24)]) == 72
assert parse_bom_line("F-101,down tube,1,meters,155") == BomLine(
    "F-101", "down tube", 1, "meters", Pence(155)
)
assert parse_bom_line("F-101,down tube,one,meters,155") is None
