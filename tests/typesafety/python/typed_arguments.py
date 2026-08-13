# expect: passes
# run: yes
# title: With real types, the mixed-up call fails the type check
# improves: any_arguments
def invoice_line(description: str, price_pence: int, quantity: int, taxed: bool) -> str:
    note = " +tax" if taxed else ""
    return f"{description} x{quantity} at {price_pence}d{note}"


assert invoice_line("handlebar grip", 80, 2, taxed=False) == "handlebar grip x2 at 80d"
assert invoice_line("saddle", 155, 1, taxed=True) == "saddle x1 at 155d +tax"
