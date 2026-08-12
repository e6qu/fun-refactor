// expect: passes
// An `any`-taking callback accepts any arguments at all, so a key function
// that takes the wrong thing still compiles, and fails at run time instead.

type Order = { readonly id: string; readonly totalCents: number };

function cheapestFirst(orders: Order[], key: (...args: any[]) => number): Order[] {
  return [...orders].sort((a, b) => key(a) - key(b));
}

export function demo(orders: Order[]): Order[] {
  // The checker accepts this key, and every call of it returns NaN at run
  // time: an Order has no `[currency]`.
  return cheapestFirst(orders, (order, currency) => Number(order[currency]));
}
