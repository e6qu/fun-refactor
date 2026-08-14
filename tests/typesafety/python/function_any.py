# expect: passes
# title: A loosely typed key accepts a lambda the sort cannot call
from collections.abc import Callable
from dataclasses import dataclass
from typing import NewType

Pence = NewType("Pence", int)


@dataclass(frozen=True)
class InvoiceLine:
    item: str
    pence: Pence
    quantity: int


def picking_list(
    lines: list[InvoiceLine], key: Callable[..., int]
) -> list[InvoiceLine]:
    return sorted(lines, key=key)


cheapest_first = picking_list([], lambda line, currency: line[currency])
