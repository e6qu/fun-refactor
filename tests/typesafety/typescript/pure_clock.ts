// expect: passes

export function remainingMs(deadline: Date, now: Date): number {
  return deadline.getTime() - now.getTime();
}

const dispatch = new Date("2026-08-13T16:00:00");

export const before = remainingMs(dispatch, new Date("2026-08-13T15:30:00"));
export const after = remainingMs(dispatch, new Date("2026-08-13T16:45:00"));
