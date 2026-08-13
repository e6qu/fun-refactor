// expect: passes

function transfer(fromAccount: string, toAccount: string, amountCents: number): string {
  return `move ${amountCents} from ${fromAccount} to ${toAccount}`;
}

export function payRent(tenantAccount: string, landlordAccount: string): string {
  // The arguments are in the wrong order. The checker accepts this call,
  // because both parameters have the same type. The money goes the wrong way.
  return transfer(landlordAccount, tenantAccount, 95_000);
}
