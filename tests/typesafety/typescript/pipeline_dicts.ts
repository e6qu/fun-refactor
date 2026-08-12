// expect: passes
// The steps pass a loose record along. Each one checks the keys it needs, and
// a step out of order fails at run time, when it fails at all.

function parseOrder(raw: Record<string, unknown>): Record<string, unknown> {
  const quantityText = raw["quantityText"];
  if (typeof quantityText !== "string") {
    throw new Error("quantityText missing");
  }
  return { item: raw["item"], quantity: Number(quantityText) };
}

function price(order: Record<string, unknown>, unitCents: number): Record<string, unknown> {
  const quantity = order["quantity"];
  if (typeof quantity !== "number") {
    throw new Error("quantity missing");
  }
  return { item: order["item"], totalCents: quantity * unitCents };
}

export function quote(raw: Record<string, unknown>): Record<string, unknown> {
  return price(parseOrder(raw), 250);
}
