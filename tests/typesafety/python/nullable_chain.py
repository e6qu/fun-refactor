# expect: passes
# title: quote returns None for three different reasons
def parse_quantity(text: str) -> int | None:
    return int(text) if text.isdigit() else None


def check_stock(quantity: int) -> int | None:
    return quantity if quantity <= 10 else None


def quote(text: str) -> int | None:
    quantity = parse_quantity(text)
    if quantity is None:
        return None
    in_stock = check_stock(quantity)
    if in_stock is None:
        return None
    return in_stock * 250
