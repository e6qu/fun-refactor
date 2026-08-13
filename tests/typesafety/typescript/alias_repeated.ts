// expect: passes

export const fixedBackoff: (attempt: number, error: Error) => number = () => 30_000;

export const doublingBackoff: (attempt: number, error: Error) => number = (attempt) =>
  30_000 * 2 ** attempt;

export function runWithRetries(policy: (attempt: number, error: Error) => number): number {
  return policy(1, new Error("transient"));
}
