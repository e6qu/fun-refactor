// expect: passes

export function shout(note: string | null): string {
  return (note as string).toUpperCase();
}
