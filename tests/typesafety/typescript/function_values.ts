// expect: passes

type Order = {
  readonly id: string;
  readonly totalCents: number;
};

function cheapestFirst(orders: readonly Order[], key: (order: Order) => number): Order[] {
  return [...orders].sort((a, b) => key(a) - key(b));
}

export function demo(orders: Order[]): Order[] {
  return cheapestFirst(orders, (order) => order.totalCents);
}
