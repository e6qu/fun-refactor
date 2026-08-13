// expect: passes

type RawOrder = { readonly item: string; readonly quantityText: string };
type Order = { readonly item: string; readonly quantity: number };
type Priced = { readonly item: string; readonly totalCents: number };

function parseOrder(raw: RawOrder): Order {
  const quantity = Number(raw.quantityText);
  if (!Number.isInteger(quantity)) {
    throw new Error(`quantity is not a number: ${raw.quantityText}`);
  }
  return { item: raw.item, quantity };
}

function price(order: Order, unitCents: number): Priced {
  return { item: order.item, totalCents: order.quantity * unitCents };
}

export function quote(raw: RawOrder): Priced {
  return price(parseOrder(raw), 250);
}
