// expect: passes

function refund(sourceAccount: string, targetAccount: string, amountCents: number): string {
  return `move ${amountCents} from ${sourceAccount} to ${targetAccount}`;
}

export function refundSupplier(shopAccount: string, supplierAccount: string): string {
  // The arguments are in the wrong order. The checker accepts the call,
  // because both parameters are the same type, and the money flows into
  // the shop instead of back to the supplier.
  return refund(supplierAccount, shopAccount, 4_500);
}
