// expect: fails
// The same wrong call as the any version. The checker rejects it during its
// scan; nothing has to run to find the mistake.

function orderLine(name: string, unitPrice: number, quantity: number, gift: boolean): string {
  const note = gift ? " (gift)" : "";
  return `${name} x${quantity} at ${unitPrice.toFixed(2)}${note}`;
}

export const line = orderLine(3, "tea", true, 1.95); // rejected by the checker
