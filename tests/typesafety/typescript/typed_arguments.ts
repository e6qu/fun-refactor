// expect: passes

function orderLine(name: string, unitPrice: number, quantity: number, gift: boolean): string {
  const note = gift ? " (gift)" : "";
  return `${name} x${quantity} at ${unitPrice.toFixed(2)}${note}`;
}

export const tea = orderLine("tea", 1.95, 3, false); // "tea x3 at 1.95"
export const mug = orderLine("mug", 8.0, 1, true); // "mug x1 at 8.00 (gift)"
