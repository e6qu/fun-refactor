"""Pinned investigation subject; the initial discount implementation is defective."""

def subtotal(price, count):
    return price * count


def discount(amount, member):
    if member:
        return amount - 10
    return amount


def checkout(price, count, member):
    return discount(subtotal(price, count), member)
