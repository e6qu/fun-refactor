# expect: fails
# title: The bill of materials and the invoice, as they stand today


def bom_line(part_no, description, qty, unit, cost):
    return f"{part_no}  {description}  {qty} {unit} at {cost}d"


def product_cost(costs_pounds):
    return sum(costs_pounds)


def invoice_line(description, price_pence, quantity, taxed):
    note = " +tax" if taxed else ""
    return f"{description} x{quantity} at {price_pence}d{note}"


def invoice_total(prices_pounds):
    return sum(prices_pounds)


def apply_discount(total_pounds, rate):
    return total_pounds * (1 - rate)


def advance(status):
    if status == "darft":
        return "sent"
    if status == "sent":
        return "paid"
    return status


def bill(customer_id, product_id):
    return f"invoice {customer_id} for one {product_id}"


def load_bom_line(row):
    return row.split(",")
