// expect: passes
// The address travels as a plain string, so each function checks it again.
// No function can trust that another already did.

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
