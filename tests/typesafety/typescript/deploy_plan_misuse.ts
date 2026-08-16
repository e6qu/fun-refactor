// expect: fails

declare const portBrand: unique symbol;
type Port = number & { readonly [portBrand]: true };

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

type Resource = {
  readonly kind: string;
  readonly name: string;
  readonly attributes: { [key: string]: Json };
};

type ServerPlan = { readonly name: string; readonly port: Port; readonly replicas: number };

function summary(plan: ServerPlan): string {
  return `${plan.name} on ${plan.port} x${plan.replicas}`;
}

export function report(resource: Resource): string {
  return summary(resource);
}
