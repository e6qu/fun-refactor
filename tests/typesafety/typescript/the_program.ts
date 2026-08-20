// expect: fails

export function bomLine(partNo, description, qty, unit, cost) {
  return `${partNo}  ${description}  ${qty} ${unit} at ${cost}d`;
}

export function productCost(costsPounds) {
  return costsPounds.reduce((sum, cost) => sum + cost, 0);
}

export function invoiceLine(description, pricePence, quantity, taxed) {
  const note = taxed ? " +tax" : "";
  return `${description} x${quantity} at ${pricePence}d${note}`;
}

export function invoiceTotal(pricesPounds) {
  return pricesPounds.reduce((sum, price) => sum + price, 0);
}

export function applyDiscount(totalPounds, rate) {
  return totalPounds * (1 - rate);
}

export function advance(status) {
  if (status === "darft") return "sent";
  if (status === "sent") return "paid";
  return status;
}

export function bill(customerId, productId) {
  return `invoice ${customerId} for one ${productId}`;
}

export function loadBomLine(row) {
  return row.split(",");
}
