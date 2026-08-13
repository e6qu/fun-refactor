# expect: passes
# title: Two look-alike ids share one type, so an invoice can go to a product
def bill(customer_id: str, product_id: str) -> str:
    return f"invoice {customer_id} for one {product_id}"


def bill_roadster(customer_id: str, product_id: str) -> str:
    return bill(product_id, customer_id)
