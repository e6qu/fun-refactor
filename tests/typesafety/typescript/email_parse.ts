// expect: passes
// `parseEmail` is the only place an `EmailAddress` is made. Any function that
// holds one knows the check already ran.

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
  // No validation here, and none needed. The type is the proof.
  return `receipt sent to ${to}`;
}

export function checkout(rawFormField: string): string {
  const email = parseEmail(rawFormField);
  if (email === null) {
    return "ask the user again";
  }
  return sendReceipt(email);
}
