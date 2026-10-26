pub fn allowed(staff: bool, owner: bool) -> bool {
    if staff {
        return true;
    }
    owner
}
