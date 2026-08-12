# expect: passes
# title: Two parameters share one type, so the checker accepts the swapped call
"""Both parameters are `str`, so swapping them type-checks."""


def transfer(from_account: str, to_account: str, amount_cents: int) -> str:
    return f"move {amount_cents} from {from_account} to {to_account}"


def pay_rent(tenant_account: str, landlord_account: str) -> str:
    # The arguments are in the wrong order. The checker accepts this call,
    # because both parameters have the same type. The money goes the wrong way.
    return transfer(landlord_account, tenant_account, 95_000)
