// expect: passes

export function advance(status: string): string {
  if (status === "darft") {
    return "sent";
  }
  if (status === "sent") {
    return "paid";
  }
  return status;
}
