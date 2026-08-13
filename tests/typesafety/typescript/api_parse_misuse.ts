// expect: fails

import { z } from "zod";

const Order = z.strictObject({
  id: z.string(),
  quantity: z.number().int(),
  giftNote: z.string().nullable(),
});

type Order = z.infer<typeof Order>;

function priceCents(order: Order): number {
  return order.quantity * 250;
}

export const total = priceCents({ id: "o1", quantity: "2", giftNote: null });
