# expect: passes
# title: Two account numbers share one type, so a refund can flow the wrong way
def refund(source_account: str, target_account: str, amount_cents: int) -> str:
    return f"move {amount_cents} from {source_account} to {target_account}"


def refund_supplier(shop_account: str, supplier_account: str) -> str:
    # The arguments are in the wrong order. The checker accepts the call,
    # because both parameters are the same type, and the money flows into
    # the shop instead of back to the supplier.
    return refund(supplier_account, shop_account, 4_500)
