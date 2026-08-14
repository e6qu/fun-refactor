# expect: fails
# title: A key that returns the item name, rejected by the checker
# misuse-of: function_values
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
    lines: list[InvoiceLine], key: Callable[[InvoiceLine], int]
) -> list[InvoiceLine]:
    return sorted(lines, key=key)


by_name = picking_list([], lambda line: line.item)
