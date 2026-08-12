# expect: fails
# title: The mixed-up call, rejected by the checker
# misuse-of: typed_arguments
"""The same wrong call as the Any version. The checker rejects it during its
scan; nothing has to run to find the mistake."""


def order_line(name: str, unit_price: float, quantity: int, gift: bool) -> str:
    note = " (gift)" if gift else ""
    return f"{name} x{quantity} at {unit_price:.2f}{note}"


line = order_line(3, "tea", True, 1.95)  # rejected by the checker
