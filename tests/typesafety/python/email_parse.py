# expect: passes
# title: The parse function proves the check ran
# improves: email_checked
"""`parse_email` is the only way to make an `EmailAddress`. Any function that
holds one knows the check already ran."""

from typing import NewType

EmailAddress = NewType("EmailAddress", str)


def parse_email(raw: str) -> EmailAddress | None:
    candidate = raw.strip().lower()
    if "@" not in candidate or candidate.startswith("@"):
        return None
    return EmailAddress(candidate)


def send_receipt(to: EmailAddress) -> str:
    # No validation here, and none needed. The type is the proof.
    return f"receipt sent to {to}"


def checkout(raw_form_field: str) -> str:
    email = parse_email(raw_form_field)
    if email is None:
        return "ask the user again"
    return send_receipt(email)
