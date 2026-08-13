# expect: fails
# title: The swapped refund, rejected by the checker
# misuse-of: entity_ids
from typing import NewType

ShopAccount = NewType("ShopAccount", str)
SupplierAccount = NewType("SupplierAccount", str)


def refund(source: ShopAccount, target: SupplierAccount, amount_cents: int) -> str:
    return f"move {amount_cents} from {source} to {target}"


def refund_supplier(shop: ShopAccount, supplier: SupplierAccount) -> str:
    return refund(supplier, shop, 4_500)  # rejected: both types are wrong
