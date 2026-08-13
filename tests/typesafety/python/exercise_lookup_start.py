# expect: passes
# title: first_item returns None for three different reasons
def find_user_id(login: str) -> str | None:
    return "u7" if login == "ada" else None


def find_cart(user_id: str) -> list[str] | None:
    return ["book"] if user_id == "u7" else None


def first_item(login: str) -> str | None:
    user_id = find_user_id(login)
    if user_id is None:
        return None
    cart = find_cart(user_id)
    if cart is None:
        return None
    if not cart:
        return None
    return cart[0]
