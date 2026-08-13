// expect: passes

export function nextAction(status: string): string {
  if (status === "recieved") {
    return "start picking";
  }
  if (status === "picked") {
    return "pack the box";
  }
  if (status === "shipped") {
    return "send the tracking mail";
  }
  return "unknown status";
}
