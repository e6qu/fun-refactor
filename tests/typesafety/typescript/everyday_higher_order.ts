// expect: passes

type Order = { readonly item: string; readonly totalCents: number; readonly gift: boolean };

export function giftSpendCents(orders: Order[]): number {
  return orders
    .filter((order) => order.gift)
    .map((order) => order.totalCents)
    .reduce((sum, cents) => sum + cents, 0);
}
