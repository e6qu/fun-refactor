// expect: passes

declare const penceBrand: unique symbol;
declare const customerBrand: unique symbol;
declare const productBrand: unique symbol;

type Pence = number & { readonly [penceBrand]: true };
type CustomerId = string & { readonly [customerBrand]: true };
type ProductId = string & { readonly [productBrand]: true };
type Status = "draft" | "sent" | "paid";
type Unit = "each" | "meters" | "kilograms";

type BomLine = {
  readonly partNo: string;
  readonly description: string;
  readonly qty: number;
  readonly unit: Unit;
  readonly cost: Pence;
};

function pence(n: number): Pence {
  return n as Pence;
}

export function invoiceLine(description: string, price: Pence, quantity: number, taxed: boolean): string {
  const note = taxed ? " +tax" : "";
  return `${description} x${quantity} at ${price}d${note}`;
}

export function invoiceTotal(prices: Pence[]): Pence {
  return pence(prices.reduce((sum, price) => sum + price, 0));
}

export function advance(status: Status): Status {
  switch (status) {
    case "draft":
      return "sent";
    case "sent":
      return "paid";
    case "paid":
      return "paid";
  }
}

export function bill(customer: CustomerId, product: ProductId): string {
  return `invoice ${customer} for one ${product}`;
}

function isUnit(value: string): value is Unit {
  return value === "each" || value === "meters" || value === "kilograms";
}

export function parseBomLine(row: string): BomLine | null {
  const fields = row.split(",");
  if (fields.length !== 5) return null;
  const [partNo, description, qtyText, unitText, costText] = fields;
  if (!/^\d+$/.test(qtyText) || !/^\d+$/.test(costText) || !isUnit(unitText)) return null;
  return { partNo, description, qty: Number(qtyText), unit: unitText, cost: pence(Number(costText)) };
}
