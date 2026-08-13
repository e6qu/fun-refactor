// expect: passes

export function advance(status: string): string {
  if (status === "darft") { // typo: never matches "draft"
    return "sent";
  }
  if (status === "sent") {
    return "paid";
  }
  return status;
}
