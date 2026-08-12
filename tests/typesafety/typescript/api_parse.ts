// expect: passes
// The request body is a string. `Order.parse` turns it into an `Order` or
// throws, once, here. Every function past this point takes an `Order`.

import { z } from "zod";

const Order = z.object({
  id: z.string(),
  quantity: z.number().int(),
  giftNote: z.string().optional(),
});

type Order = z.infer<typeof Order>;

function priceCents(order: Order): number {
  // No check that `quantity` exists, and no check that it is a number.
  // The type already says both.
  return order.quantity * 250;
}

export function handle(body: string): number {
  const order = Order.parse(JSON.parse(body));
  return priceCents(order);
}
