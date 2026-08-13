// expect: fails

declare const emailBrand: unique symbol;
type EmailAddress = string & { readonly [emailBrand]: true };

function sendReceipt(to: EmailAddress): string {
  return `receipt sent to ${to}`;
}

export const confirmation = sendReceipt("bob@example.test");
