# expect: passes
# title: Any string can claim to be a status
"""One branch has a typo, so it never matches. The checker sees only strings
and has no way to notice."""


def advance(status: str) -> str:
    if status == "darft":  # typo: never matches "draft"
        return "sent"
    if status == "sent":
        return "paid"
    return status
