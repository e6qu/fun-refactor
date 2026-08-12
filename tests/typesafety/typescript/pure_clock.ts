// expect: passes
// The function is now a table of facts. A test picks the moment and checks
// the answer, today and every day after.

export function remaining(deadline: number, now: number): number {
  return deadline - now;
}

export const checked = remaining(120, 45) === 75; // true, every day
