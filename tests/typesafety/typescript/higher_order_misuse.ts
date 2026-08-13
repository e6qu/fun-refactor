// expect: fails

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

function fetchPage(url: string, timeout: number): string {
  return `GET ${url} within ${timeout}s`;
}

const patientFetch = retry(3, fetchPage);
export const result = patientFetch(10, "https://example.test");
