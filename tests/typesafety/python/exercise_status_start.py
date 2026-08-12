# expect: passes
# title: Exercise 1, starting point
"""Order status as plain strings. The first branch has a typo, so it never
matches, and the checker has no way to notice."""


def next_action(status: str) -> str:
    if status == "recieved":  # typo: never matches "received"
        return "start picking"
    if status == "picked":
        return "pack the box"
    if status == "shipped":
        return "send the tracking mail"
    return "unknown status"
