// expect: passes
// `retry` takes a function and returns one with the same parameters and the
// same result. The type parameters carry the whole signature through.

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
export const result: string = patientFetch("https://example.test", 10);
