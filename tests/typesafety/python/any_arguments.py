# expect: passes
# title: With Any, the checker accepts the arguments in any order
from typing import Any


def invoice_line(description: Any, price_pence: Any, quantity: Any, taxed: Any) -> Any:
    note = " +tax" if taxed else ""
    return f"{description} x{quantity} at {price_pence}d{note}"


line = invoice_line(80, "handlebar grip", True, 2)
