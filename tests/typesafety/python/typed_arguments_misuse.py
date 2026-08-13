# expect: fails
# title: The mixed-up call, rejected by the checker
# misuse-of: typed_arguments
def invoice_line(description: str, price_pence: int, quantity: int, taxed: bool) -> str:
    note = " +tax" if taxed else ""
    return f"{description} x{quantity} at {price_pence}d{note}"


line = invoice_line(80, "handlebar grip", True, 2)
