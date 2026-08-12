# expect: passes
# title: Tenant and landlord accounts become different types
# improves: transfer_arguments
"""The transfer example again, with the account numbers as distinct types."""

from typing import NewType

TenantAccount = NewType("TenantAccount", str)
LandlordAccount = NewType("LandlordAccount", str)


def transfer(source: TenantAccount, target: LandlordAccount, amount_cents: int) -> str:
    return f"move {amount_cents} from {source} to {target}"


def pay_rent(tenant: TenantAccount, landlord: LandlordAccount) -> str:
    return transfer(tenant, landlord, 95_000)
