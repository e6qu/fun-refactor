// expect: passes
// A test cannot pin this function's answer down. The answer depends on when
// the test runs.

export function remaining(deadline: number): number {
  return deadline - Date.now() / 1000;
}
