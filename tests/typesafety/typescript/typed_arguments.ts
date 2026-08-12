// expect: passes
// Plain string, number and boolean say what each argument is. The misplaced
// call from before now fails during the type check, before the program runs
// at all.

function orderLine(name: string, unitPrice: number, quantity: number, gift: boolean): string {
  const note = gift ? " (gift)" : "";
  return `${name} x${quantity} at ${unitPrice.toFixed(2)}${note}`;
}

export const tea = orderLine("tea", 1.95, 3, false); // "tea x3 at 1.95"
export const mug = orderLine("mug", 8.0, 1, true); // "mug x1 at 8.00 (gift)"
