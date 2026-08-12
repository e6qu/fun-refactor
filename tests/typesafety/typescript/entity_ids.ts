// expect: passes
// The transfer example again, with the account numbers as distinct types.

declare const tenantBrand: unique symbol;
declare const landlordBrand: unique symbol;

type TenantAccount = string & { readonly [tenantBrand]: true };
type LandlordAccount = string & { readonly [landlordBrand]: true };

function transfer(source: TenantAccount, target: LandlordAccount, amountCents: number): string {
  return `move ${amountCents} from ${source} to ${target}`;
}

export function payRent(tenant: TenantAccount, landlord: LandlordAccount): string {
  return transfer(tenant, landlord, 95_000);
}
