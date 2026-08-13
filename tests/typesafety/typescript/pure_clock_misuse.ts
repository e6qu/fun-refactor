// expect: fails

export function remainingMs(deadline: Date, now: Date): number {
  return deadline.getTime() - now.getTime();
}

const dispatch = new Date("2026-08-13T16:00:00");

export const left = remainingMs(dispatch, 16.75);
