// expect: passes

declare const portBrand: unique symbol;
type Port = number & { readonly [portBrand]: true };

type ServerPlan = { readonly name: string; readonly port: Port; readonly replicas: number };

function readPlan(attributes: Record<string, unknown>): ServerPlan {
  const { name, port, replicas = 1 } = attributes;
  if (
    typeof name !== "string" ||
    typeof port !== "number" ||
    port < 1 ||
    port > 65535 ||
    typeof replicas !== "number"
  ) {
    throw new Error("bad plan");
  }
  return { name, port: port as Port, replicas };
}

export function summary(plan: ServerPlan): string {
  return `${plan.name} on ${plan.port} x${plan.replicas}`;
}

export const shop = summary(readPlan({ name: "shop", port: 8080 }));
