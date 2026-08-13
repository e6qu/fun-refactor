// expect: fails

declare const customerBrand: unique symbol;
declare const productBrand: unique symbol;

type CustomerId = string & { readonly [customerBrand]: true };
type ProductId = string & { readonly [productBrand]: true };

function bill(customer: CustomerId, product: ProductId): string {
  return `invoice ${customer} for one ${product}`;
}

export function billRoadster(customer: CustomerId, product: ProductId): string {
  return bill(product, customer); // rejected: both types are wrong
}
