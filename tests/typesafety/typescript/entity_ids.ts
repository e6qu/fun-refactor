// expect: passes

declare const shopBrand: unique symbol;
declare const supplierBrand: unique symbol;

type ShopAccount = string & { readonly [shopBrand]: true };
type SupplierAccount = string & { readonly [supplierBrand]: true };

function refund(source: ShopAccount, target: SupplierAccount, amountCents: number): string {
  return `move ${amountCents} from ${source} to ${target}`;
}

export function refundSupplier(shop: ShopAccount, supplier: SupplierAccount): string {
  return refund(shop, supplier, 4_500);
}
