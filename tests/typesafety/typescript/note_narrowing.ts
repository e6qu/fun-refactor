// expect: passes

export function shout(note: string | null): string {
  if (note === null) {
    return "";
  }
  return note.toUpperCase();
}

export const loud = shout("fragile");
export const quiet = shout(null);
