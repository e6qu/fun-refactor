def allowed(staff: bool, owner: bool) -> bool:
    if staff:
        return True
    return owner
