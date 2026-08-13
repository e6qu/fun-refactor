// expect: fails

type RawOrder = { readonly item: string; readonly quantityText: string };
type Order = { readonly item: string; readonly quantity: number };
type Priced = { readonly item: string; readonly totalCents: number };

function price(order: Order, unitCents: number): Priced {
  return { item: order.item, totalCents: order.quantity * unitCents };
}

export const quoted = price({ item: "saddle", quantityText: "2" }, 250);
