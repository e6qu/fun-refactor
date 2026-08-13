# expect: fails
# title: The swapped bill, rejected by the checker
# misuse-of: entity_ids
from typing import NewType

CustomerId = NewType("CustomerId", str)
ProductId = NewType("ProductId", str)


def bill(customer: CustomerId, product: ProductId) -> str:
    return f"invoice {customer} for one {product}"


def bill_roadster(customer: CustomerId, product: ProductId) -> str:
    return bill(product, customer)  # rejected: both types are wrong
