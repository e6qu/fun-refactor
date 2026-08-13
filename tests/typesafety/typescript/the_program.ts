// expect: passes

export function bomLine(partNo: any, description: any, qty: any, unit: any, cost: any): any {
  return `${partNo}  ${description}  ${qty} ${unit} at ${cost}d`;
}

export function productCost(costsPounds: number[]): number {
  return costsPounds.reduce((sum, cost) => sum + cost, 0);
}

export function invoiceLine(description: any, pricePence: any, quantity: any, taxed: any): any {
  const note = taxed ? " +tax" : "";
  return `${description} x${quantity} at ${pricePence}d${note}`;
}

export function invoiceTotal(pricesPounds: number[]): number {
  return pricesPounds.reduce((sum, price) => sum + price, 0);
}

export function applyDiscount(totalPounds: number, rate: number): number {
  return totalPounds * (1 - rate);
}

export function advance(status: string): string {
  if (status === "darft") {
    return "sent";
  }
  if (status === "sent") {
    return "paid";
  }
  return status;
}

export function bill(customerId: string, productId: string): string {
  return `invoice ${customerId} for one ${productId}`;
}

export function loadBomLine(row: string): string[] {
  return row.split(",");
}
