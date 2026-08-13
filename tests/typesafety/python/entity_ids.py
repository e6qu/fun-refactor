# expect: passes
# title: Shop and supplier accounts become different types
# improves: transfer_arguments
from typing import NewType

ShopAccount = NewType("ShopAccount", str)
SupplierAccount = NewType("SupplierAccount", str)


def refund(source: ShopAccount, target: SupplierAccount, amount_cents: int) -> str:
    return f"move {amount_cents} from {source} to {target}"


def refund_supplier(shop: ShopAccount, supplier: SupplierAccount) -> str:
    return refund(shop, supplier, 4_500)
