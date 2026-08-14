// expect: passes

class ConnectionLost extends Error {}

function retry<A extends unknown[], R>(
  times: number,
  operation: (...args: A) => R,
): (...args: A) => R {
  return (...args: A): R => {
    let failures = 0;
    for (;;) {
      try {
        return operation(...args);
      } catch (error) {
        if (!(error instanceof ConnectionLost)) throw error;
        failures += 1;
        if (failures >= times) throw error;
      }
    }
  };
}

function fetch(url: string, timeout: number): string {
  return `GET ${url} within ${timeout}s`;
}

const patientFetch = retry(3, fetch);
export const result: string = patientFetch("https://example.test", 10);
