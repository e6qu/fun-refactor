// expect: passes

type Order = { readonly id: string; readonly totalCents: number };

function cheapestFirst(orders: Order[], key: (...args: any[]) => number): Order[] {
  return [...orders].sort((a, b) => key(a) - key(b));
}

export function demo(orders: Order[]): Order[] {
  // accepted, and every call returns NaN: an Order has no [currency]
  return cheapestFirst(orders, (order, currency) => Number(order[currency]));
}
