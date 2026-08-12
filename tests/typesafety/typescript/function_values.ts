// expect: passes
// `key` is a function from Order to number, and the parameter type writes
// that down. The compiler checks the arrow function against it.

type Order = {
  readonly id: string;
  readonly totalCents: number;
};

function cheapestFirst(orders: Order[], key: (order: Order) => number): Order[] {
  return [...orders].sort((a, b) => key(a) - key(b));
}

export function demo(orders: Order[]): Order[] {
  return cheapestFirst(orders, (order) => order.totalCents);
}
