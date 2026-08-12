// expect: passes
// Every step can come back empty. `| null` makes each caller decide what
// that means. Optional chaining keeps the ladder short, and it still ends
// in one undistinguished absence.

type Customer = { readonly referrerId: string | null };

function findCustomer(customerId: string): Customer | null {
  return customerId === "c1" ? { referrerId: "c2" } : null;
}

export function referrerOf(customerId: string): Customer | null {
  const referrerId = findCustomer(customerId)?.referrerId;
  return referrerId == null ? null : findCustomer(referrerId);
}
