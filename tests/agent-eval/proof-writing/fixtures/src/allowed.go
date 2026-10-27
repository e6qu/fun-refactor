package access

func allowed(staff bool, owner bool) bool {
    if staff {
        return true
    }
    return owner
}
