// expect: passes
// `remainingImpure` reads the clock itself, so its answer changes between
// calls and a test cannot pin it down. `remaining` takes the moment as an
// argument: same input, same output, every time.

export function remainingImpure(deadline: number): number {
  return deadline - Date.now() / 1000; // a different answer every call
}

export function remaining(deadline: number, now: number): number {
  return deadline - now;
}

// The Python twin runs these; the arithmetic is the same.
export const checked = remaining(120, 45) === 75;
