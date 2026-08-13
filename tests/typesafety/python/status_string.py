# expect: passes
# title: The typo never matches, and the checker cannot see it
def advance(status: str) -> str:
    if status == "darft":
        return "sent"
    if status == "sent":
        return "paid"
    return status
