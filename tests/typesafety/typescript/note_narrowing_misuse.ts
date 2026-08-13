// expect: fails

export function shout(note: string | null): string {
  return note.toUpperCase();
}
