// expect: passes

type Seconds = number;

function waitBeforeRetry(delay: Seconds): string {
  return `sleeping ${delay}s`;
}

export function plan(): string {
  const minutes = 5;
  return waitBeforeRetry(minutes); // accepted: Seconds and number are the same type
}
