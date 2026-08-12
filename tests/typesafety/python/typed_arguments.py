# expect: passes
# run: yes
# title: With real types, the mixed-up call fails the type check
# improves: any_arguments
"""Plain `str`, `float`, `int` and `bool` say what each argument is. The
misplaced call from before now fails during the type check, before the
program runs at all."""


def order_line(name: str, unit_price: float, quantity: int, gift: bool) -> str:
    note = " (gift)" if gift else ""
    return f"{name} x{quantity} at {unit_price:.2f}{note}"


assert order_line("tea", 1.95, 3, gift=False) == "tea x3 at 1.95"
assert order_line("mug", 8.00, 1, gift=True) == "mug x1 at 8.00 (gift)"
