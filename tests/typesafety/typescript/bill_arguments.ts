// expect: passes

function bill(customerId: string, productId: string): string {
  return `invoice ${customerId} for one ${productId}`;
}

export function billRoadster(customerId: string, productId: string): string {
  // The arguments are in the wrong order. The checker accepts the call,
  // because both parameters are the same type, and the invoice goes out
  // addressed to a bicycle.
  return bill(productId, customerId);
}
