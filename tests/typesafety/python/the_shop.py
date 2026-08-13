# expect: passes
# title: The shop's order system, as it stands today
from typing import Any


def order_line(name: Any, unit_price: Any, quantity: Any, gift: Any) -> Any:
    note = " (gift)" if gift else ""
    return f"{name} x{quantity} at {unit_price:.2f}{note}"


def order_total(prices: list[float]) -> float:
    return sum(prices)


def advance(status: str) -> str:
    if status == "darft":  # one of these strings is misspelled
        return "sent"
    if status == "sent":
        return "paid"
    return status


def refund(source_account: str, target_account: str, amount_cents: int) -> str:
    return f"move {amount_cents} from {source_account} to {target_account}"


def start(argv: list[str]) -> str:
    port = argv[0]  # "8080", and it stays text
    return f"listening on {port}"
