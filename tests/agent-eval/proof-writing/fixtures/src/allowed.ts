export function allowed(staff: boolean, owner: boolean): boolean {
    if (staff) {
        return true;
    }
    return owner;
}
