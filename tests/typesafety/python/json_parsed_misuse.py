# expect: fails
# title: A Json value handed straight to invoice_total, rejected by the checker
# misuse-of: json_parsed
from dataclasses import dataclass
from typing import NewType

type Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]

Pence = NewType("Pence", int)


@dataclass(frozen=True)
class Line:
    item: str
    pence: Pence
    quantity: int


def invoice_total(lines: list[Line]) -> Pence:
    return Pence(sum(line.pence * line.quantity for line in lines))


def report(decoded: Json) -> Pence:
    return invoice_total(decoded)
