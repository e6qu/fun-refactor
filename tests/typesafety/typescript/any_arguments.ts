// expect: passes
// With `any`, the checker accepts every call. The mistakes stay hidden until
// the program runs, and then surface as mangled output or a crash.

function orderLine(name: any, unitPrice: any, quantity: any, gift: any): any {
  const note = gift ? " (gift)" : "";
  return `${name} x${quantity} at ${unitPrice.toFixed(2)}${note}`;
}

// Every argument is in the wrong place. The checker accepts this call, and it
// fails at run time, when toFixed meets a string.
export const line = orderLine(3, "tea", true, 1.95);
