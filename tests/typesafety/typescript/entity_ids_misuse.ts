// expect: fails

declare const tenantBrand: unique symbol;
declare const landlordBrand: unique symbol;

type TenantAccount = string & { readonly [tenantBrand]: true };
type LandlordAccount = string & { readonly [landlordBrand]: true };

function transfer(source: TenantAccount, target: LandlordAccount, amountCents: number): string {
  return `move ${amountCents} from ${source} to ${target}`;
}

export function payRent(tenant: TenantAccount, landlord: LandlordAccount): string {
  return transfer(landlord, tenant, 95_000); // rejected: both types are wrong
}
