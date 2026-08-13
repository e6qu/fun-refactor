// expect: passes

declare const emailBrand: unique symbol;
type EmailAddress = string & { readonly [emailBrand]: true };

function parseEmail(raw: string): EmailAddress | null {
  const candidate = raw.trim();
  const at = candidate.indexOf("@");
  const local = candidate.slice(0, at < 0 ? 0 : at);
  const domain = candidate.slice(at + 1);
  if (at < 0 || !local || domain.includes("@") || !domain.includes(".")) {
    return null;
  }
  return candidate as EmailAddress;
}

function sendReceipt(to: EmailAddress): string {
  // No validation here: parse_email already said yes.
  return `receipt sent to ${to}`;
}

export function checkout(rawFormField: string): string {
  const email = parseEmail(rawFormField);
  if (email === null) {
    return "ask the user again";
  }
  return sendReceipt(email);
}
