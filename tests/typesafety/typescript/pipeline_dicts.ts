// expect: passes

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
