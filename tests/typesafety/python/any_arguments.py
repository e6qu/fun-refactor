# expect: passes
# title: With Any, the checker accepts the arguments in any order
from typing import Any


def order_line(name: Any, unit_price: Any, quantity: Any, gift: Any) -> Any:
    note = " (gift)" if gift else ""
    return f"{name} x{quantity} at {unit_price:.2f}{note}"


line = order_line(3, "tea", True, 1.95)  # accepted, and fails at run time
