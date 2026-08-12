# expect: passes
# title: Every reader of the address repeats the check
"""The address travels as a plain string, so each function checks it again.
No function can trust that another already did."""


def looks_like_email(raw: str) -> bool:
    return "@" in raw and not raw.startswith("@")


def send_receipt(to: str) -> str:
    if not looks_like_email(to):
        raise ValueError("bad address")
    return f"receipt sent to {to}"


def send_reminder(to: str) -> str:
    if not looks_like_email(to):  # the same check, again
        raise ValueError("bad address")
    return f"reminder sent to {to}"
