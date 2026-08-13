// expect: passes

function looksLikeEmail(raw: string): boolean {
  return raw.includes("@") && !raw.startsWith("@");
}

export function sendReceipt(to: string): string {
  if (!looksLikeEmail(to)) {
    throw new Error("bad address");
  }
  return `receipt sent to ${to}`;
}

export function sendReminder(to: string): string {
  if (!looksLikeEmail(to)) { // the same check, again
    throw new Error("bad address");
  }
  return `reminder sent to ${to}`;
}
