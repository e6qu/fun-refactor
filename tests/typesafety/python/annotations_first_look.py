# expect: passes
# title: A function with its types written out
def shelf_label(item: str, price: float) -> str:
    return f"{item}: {price:.2f} EUR"


label: str = shelf_label("tea", 4.50)
