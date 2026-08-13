// expect: passes

function priceCents(order: Record<string, unknown>): number {
  const quantity = order["quantity"];
  if (typeof quantity !== "number") {
    throw new Error("quantity missing or not a number");
  }
  return quantity * 250;
}

export function handle(body: string): number {
  const order: unknown = JSON.parse(body);
  if (typeof order !== "object" || order === null) {
    throw new Error("not an object");
  }
  return priceCents(order as Record<string, unknown>);
}
