// expect: passes

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

type Resource = {
  readonly kind: string;
  readonly name: string;
  readonly attributes: { [key: string]: Json };
};

function portOf(resource: Resource): number {
  return resource.attributes.port as number;
}

function replicasOf(resource: Resource): number {
  return (resource.attributes.replicas ?? 1) as number;
}

export function summary(resource: Resource): string {
  return `${resource.name} on ${portOf(resource)} x${replicasOf(resource)}`;
}
