# expect: passes
# title: A loop accumulates the gift total by hand
from dataclasses import dataclass
from typing import NewType

Pence = NewType("Pence", int)


@dataclass(frozen=True)
class InvoiceLine:
    item: str
    pence: Pence
    gift: bool


def gift_total(lines: list[InvoiceLine]) -> Pence:
    total = 0
    for line in lines:
        if line.gift:
            total += line.pence
    return Pence(total)
