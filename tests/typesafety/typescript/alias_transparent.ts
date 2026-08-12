// expect: passes
// `type Seconds = number` documents the parameter. It is still `number` to
// the checker.

type Seconds = number;

function waitBeforeRetry(delay: Seconds): string {
  return `sleeping ${delay}s`;
}

export function plan(): string {
  const minutes = 5;
  // The checker accepts this call. The alias and number are the same type,
  // so nothing points out that these are minutes.
  return waitBeforeRetry(minutes);
}
