// expect: passes

function parseQuantity(text: string): number | null {
  return /^\d+$/.test(text) ? Number(text) : null;
}

function checkStock(quantity: number): number | null {
  return quantity <= 10 ? quantity : null;
}

export function quote(text: string): number | null {
  const quantity = parseQuantity(text);
  if (quantity === null) {
    return null;
  }
  const inStock = checkStock(quantity);
  if (inStock === null) {
    return null;
  }
  return inStock * 250;
}
