// expect: passes

export function remaining(deadline: number): number {
  return deadline - Date.now() / 1000;
}
