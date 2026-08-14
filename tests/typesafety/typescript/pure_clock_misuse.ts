// expect: fails

export function remaining(deadline: Date, now: Date): number {
  return deadline.getTime() - now.getTime();
}

const dispatch = new Date("2026-08-13T16:00:00");

export const left = remaining(dispatch, 16.75);
