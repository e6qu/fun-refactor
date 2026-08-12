// expect: passes
// An alias earns its keep on a compound type: the name reads, and one edit
// changes every signature. A named constant does the same for a magic number.

type Milliseconds = number;
type RetryPolicy = (attempt: number, error: Error) => Milliseconds;

const DEFAULT_BACKOFF: Milliseconds = 30_000;

export const fixedBackoff: RetryPolicy = () => DEFAULT_BACKOFF;

export const doublingBackoff: RetryPolicy = (attempt) => DEFAULT_BACKOFF * 2 ** attempt;

export function runWithRetries(policy: RetryPolicy): Milliseconds {
  return policy(1, new Error("transient"));
}
