// expect: passes
// The loop filters, transforms and sums by hand, in one mutable pass. Each new
// question about the orders gets another loop like it.

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
