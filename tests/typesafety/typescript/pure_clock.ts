// expect: passes

export function remaining(deadline: Date, now: Date): number {
  return deadline.getTime() - now.getTime();
}

const dispatch = new Date("2026-08-13T16:00:00");

export const before = remaining(dispatch, new Date("2026-08-13T15:30:00"));
export const after = remaining(dispatch, new Date("2026-08-13T16:45:00"));
