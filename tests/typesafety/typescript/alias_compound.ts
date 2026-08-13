// expect: passes

type Milliseconds = number;
type RetryPolicy = (attempt: number, error: Error) => Milliseconds;

const DEFAULT_BACKOFF: Milliseconds = 30_000;

export const fixedBackoff: RetryPolicy = () => DEFAULT_BACKOFF;

export const doublingBackoff: RetryPolicy = (attempt) => DEFAULT_BACKOFF * 2 ** attempt;

export function runWithRetries(policy: RetryPolicy): Milliseconds {
  return policy(1, new Error("transient"));
}
