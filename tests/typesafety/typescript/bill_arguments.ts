// expect: passes

function bill(customerId: string, productId: string): string {
  return `invoice ${customerId} for one ${productId}`;
}

export function billRoadster(customerId: string, productId: string): string {
  return bill(productId, customerId);
}
