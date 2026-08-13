// expect: passes

declare const penceBrand: unique symbol;
declare const rateBrand: unique symbol;
declare const customerBrand: unique symbol;
declare const productBrand: unique symbol;

type Pence = number & { readonly [penceBrand]: true };
type Rate = number & { readonly [rateBrand]: true };
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

export function invoiceTotal(prices: readonly Pence[]): Pence {
  return pence(prices.reduce((sum, price) => sum + price, 0));
}

function rate(n: number): Rate {
  return n as Rate;
}

export function applyDiscount(total: Pence, discount: Rate): Pence {
  return pence(Math.round(total * (1 - discount)));
}

export const discounted = applyDiscount(pence(1250), rate(0.1));

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

function isRow(fields: string[]): fields is [string, string, string, string, string] {
  return fields.length === 5;
}

export function parseBomLine(row: string): BomLine | null {
  const fields = row.split(",");
  if (!isRow(fields)) return null;
  const [partNo, description, qtyText, unitText, costText] = fields;
  if (!/^\d+$/.test(qtyText) || !/^\d+$/.test(costText) || !isUnit(unitText)) return null;
  return { partNo, description, qty: Number(qtyText), unit: unitText, cost: pence(Number(costText)) };
}
