// expect: passes

type Meters = number;

function cutTubing(length: Meters): string {
  return `cutting ${length}m of tubing`;
}

export function restock(): string {
  const spokes = 36;
  return cutTubing(spokes); // accepted: Meters and number are the same type
}
