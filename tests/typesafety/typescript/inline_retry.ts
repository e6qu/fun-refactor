// expect: passes

function unreliableFetch(): string {
  throw new Error("try again");
}

export function fetchGreeting(attemptsLeft = 3): string {
  let failures = 0;
  for (;;) {
    try {
      return unreliableFetch().toUpperCase();
    } catch (error) {
      failures += 1;
      if (failures >= attemptsLeft) throw error;
    }
  }
}
