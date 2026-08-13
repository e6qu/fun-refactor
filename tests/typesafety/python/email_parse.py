# expect: passes
# title: An EmailAddress records that parse_email accepted it
# improves: email_checked
from typing import NewType

EmailAddress = NewType("EmailAddress", str)


def parse_email(raw: str) -> EmailAddress | None:
    candidate = raw.strip()
    local, sep, domain = candidate.partition("@")
    if not sep or not local or "@" in domain or "." not in domain:
        return None
    return EmailAddress(candidate)


def send_receipt(to: EmailAddress) -> str:
    return f"receipt sent to {to}"


def checkout(raw_form_field: str) -> str:
    email = parse_email(raw_form_field)
    if email is None:
        return "ask the user again"
    return send_receipt(email)
