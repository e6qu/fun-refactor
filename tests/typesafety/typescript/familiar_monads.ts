// expect: passes

type Assembly = { readonly name: string; readonly parts: readonly string[] };

export function allParts(assemblies: readonly Assembly[]): string[] {
  return assemblies.flatMap((assembly) => [...assembly.parts]);
}

export async function quotedTotal(fetch: (part: string) => Promise<number>): Promise<number> {
  const frame = await fetch("F-101");
  const wheels = await fetch("W-200");
  return frame + wheels;
}

export function noteLength(note: string | null): number {
  return note?.length ?? 0;
}
