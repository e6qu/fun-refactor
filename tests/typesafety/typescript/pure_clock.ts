// expect: passes

export function remaining(deadline: number, now: number): number {
  return deadline - now;
}

export const checked = remaining(120, 45) === 75; // true, every day
