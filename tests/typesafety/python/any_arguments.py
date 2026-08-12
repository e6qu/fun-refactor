# expect: passes
# title: With Any, the checker accepts the arguments in any order
"""With `Any`, the checker accepts every call. The mistakes stay hidden until
the program runs, and then surface as mangled output or a crash."""

from typing import Any


def order_line(name: Any, unit_price: Any, quantity: Any, gift: Any) -> Any:
    note = " (gift)" if gift else ""
    return f"{name} x{quantity} at {unit_price:.2f}{note}"


# Every argument is in the wrong place. The checker accepts this call, and it
# fails at run time, when the format meets a string.
line = order_line(3, "tea", True, 1.95)
