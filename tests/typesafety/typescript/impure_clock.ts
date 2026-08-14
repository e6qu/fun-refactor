// expect: passes

export function remaining(deadline: Date): number {
  return deadline.getTime() - Date.now();
}
