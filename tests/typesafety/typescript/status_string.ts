// expect: passes
// One branch has a typo, so it never matches. The checker sees only strings
// and has no way to notice.

export function advance(status: string): string {
  if (status === "darft") { // typo: never matches "draft"
    return "sent";
  }
  if (status === "sent") {
    return "paid";
  }
  return status;
}
