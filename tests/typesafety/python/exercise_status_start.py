# expect: passes
# title: The typo makes a branch unreachable
def next_action(status: str) -> str:
    if status == "recieved":  # typo: never matches "received"
        return "start picking"
    if status == "picked":
        return "pack the box"
    if status == "shipped":
        return "send the tracking mail"
    return "unknown status"
