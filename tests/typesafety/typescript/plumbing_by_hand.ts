// expect: passes

type Order = { readonly item: string; readonly totalCents: number; readonly gift: boolean };

export function giftSpendCents(orders: Order[]): number {
  let sum = 0;
  for (const order of orders) {
    if (order.gift) {
      sum += order.totalCents;
    }
  }
  return sum;
}
