// expect: passes

class ConnectionLost extends Error {}

function unreliableFetch(): string {
  throw new ConnectionLost("try again");
}

export function fetchGreeting(attemptsLeft = 3): string {
  let failures = 0;
  for (;;) {
    try {
      return unreliableFetch().toUpperCase();
    } catch (error) {
      if (!(error instanceof ConnectionLost)) throw error;
      failures += 1;
      if (failures >= attemptsLeft) throw error;
    }
  }
}
