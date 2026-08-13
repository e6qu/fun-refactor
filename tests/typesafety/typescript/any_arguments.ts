// expect: passes

function orderLine(name: any, unitPrice: any, quantity: any, gift: any): any {
  const note = gift ? " (gift)" : "";
  return `${name} x${quantity} at ${unitPrice.toFixed(2)}${note}`;
}

export const line = orderLine(3, "tea", true, 1.95); // accepted, and fails at run time
