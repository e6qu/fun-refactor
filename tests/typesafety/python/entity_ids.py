# expect: passes
# title: Customers and products become different types
# improves: bill_arguments
from typing import NewType

CustomerId = NewType("CustomerId", str)
ProductId = NewType("ProductId", str)


def bill(customer: CustomerId, product: ProductId) -> str:
    return f"invoice {customer} for one {product}"


def bill_roadster(customer: CustomerId, product: ProductId) -> str:
    return bill(customer, product)
