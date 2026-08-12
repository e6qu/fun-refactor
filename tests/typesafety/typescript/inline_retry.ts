// expect: passes
// The connection policy and the business logic share one body. The next
// function that talks to the network pastes the same loop again.

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
