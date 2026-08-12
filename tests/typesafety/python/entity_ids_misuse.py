# expect: fails
# title: The swapped transfer fails to compile
# misuse-of: entity_ids
"""The bug from the first section, written against the typed accounts."""

from typing import NewType

TenantAccount = NewType("TenantAccount", str)
LandlordAccount = NewType("LandlordAccount", str)


def transfer(source: TenantAccount, target: LandlordAccount, amount_cents: int) -> str:
    return f"move {amount_cents} from {source} to {target}"


def pay_rent(tenant: TenantAccount, landlord: LandlordAccount) -> str:
    # The same swapped arguments. The checker rejects both of them.
    return transfer(landlord, tenant, 95_000)
