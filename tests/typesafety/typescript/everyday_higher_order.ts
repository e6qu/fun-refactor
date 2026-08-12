// expect: passes
// map, filter and reduce are JavaScript's everyday higher-order functions.
// Each takes a typed callback, and the types flow through the chain: Order[]
// into number[], into number. The checker reads every arrow along the way.

type Order = { readonly item: string; readonly totalCents: number; readonly gift: boolean };

export function giftSpendCents(orders: Order[]): number {
  return orders
    .filter((order) => order.gift)
    .map((order) => order.totalCents)
    .reduce((sum, cents) => sum + cents, 0);
}
