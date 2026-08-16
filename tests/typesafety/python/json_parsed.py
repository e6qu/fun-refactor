# expect: passes
# run: yes
# title: One parse at the door, and the Json type stops at it
# improves: json_blob
from dataclasses import dataclass
from typing import NewType

Pence = NewType("Pence", int)


@dataclass(frozen=True)
class Line:
    item: str
    pence: Pence
    quantity: int


def parse_line(raw: object) -> Line:
    match raw:
        case {"item": str(item), "pence": int(pence), "quantity": int(quantity)}:
            return Line(item, Pence(pence), quantity)
        case _:
            raise ValueError(f"not a line: {raw!r}")


def line_total(line: Line) -> Pence:
    return Pence(line.pence * line.quantity)


def invoice_total(lines: list[Line]) -> Pence:
    return Pence(sum(line_total(line) for line in lines))


def is_large(lines: list[Line]) -> bool:
    return invoice_total(lines) > 1000


basket = [parse_line({"item": "saddle", "pence": 155, "quantity": 4})]
assert invoice_total(basket) == 620
assert is_large(basket) is False
