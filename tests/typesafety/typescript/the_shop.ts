// expect: passes

export function orderLine(name: any, unitPrice: any, quantity: any, gift: any): any {
  const note = gift ? " (gift)" : "";
  return `${name} x${quantity} at ${unitPrice.toFixed(2)}${note}`;
}

export function orderTotal(prices: number[]): number {
  return prices.reduce((sum, price) => sum + price, 0);
}

export function advance(status: string): string {
  if (status === "darft") {
    // one of these strings is misspelled
    return "sent";
  }
  if (status === "sent") {
    return "paid";
  }
  return status;
}

export function refund(sourceAccount: string, targetAccount: string, amountCents: number): string {
  return `move ${amountCents} from ${sourceAccount} to ${targetAccount}`;
}

export function start(argv: string[]): string {
  const port = argv[0]; // "8080", and it stays text
  return `listening on ${port}`;
}
