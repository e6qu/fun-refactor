# expect: fails
# title: A raw string where an EmailAddress is required, rejected by the checker
# misuse-of: email_parse
from typing import NewType

EmailAddress = NewType("EmailAddress", str)


def send_receipt(to: EmailAddress) -> str:
    return f"receipt sent to {to}"


confirmation = send_receipt("bob@example.test")
