# expect: passes
# title: Both senders check the address again
def looks_like_email(raw: str) -> bool:
    return "@" in raw and not raw.startswith("@")


def send_receipt(to: str) -> str:
    if not looks_like_email(to):
        raise ValueError("bad address")
    return f"receipt sent to {to}"


def send_reminder(to: str) -> str:
    if not looks_like_email(to):
        raise ValueError("bad address")
    return f"reminder sent to {to}"
