// expect: passes

export function remainingMs(deadline: Date): number {
  return deadline.getTime() - Date.now();
}
