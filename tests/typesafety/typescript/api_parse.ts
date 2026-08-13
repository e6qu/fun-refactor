// expect: passes

import { z } from "zod";

const Order = z.object({
  id: z.string(),
  quantity: z.number().int(),
  giftNote: z.string().nullable(),
});

type Order = z.infer<typeof Order>;

function priceCents(order: Order): number {
  return order.quantity * 250; // no checks: the type says both
}

export function handle(body: string): number {
  const order = Order.parse(JSON.parse(body));
  return priceCents(order);
}
