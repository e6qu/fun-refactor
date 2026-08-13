// expect: passes

declare const emailBrand: unique symbol;
type EmailAddress = string & { readonly [emailBrand]: true };

function parseEmail(raw: string): EmailAddress | null {
  const candidate = raw.trim().toLowerCase();
  if (!candidate.includes("@") || candidate.startsWith("@")) {
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
