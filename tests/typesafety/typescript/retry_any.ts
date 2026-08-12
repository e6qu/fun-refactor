// expect: passes
// `retry` returns an `any`-shaped function, so the checker no longer sees the
// parameters or the result of the function inside it.

function retry(times: number, operation: (...args: any[]) => any): (...args: any[]) => any {
  return (...args: any[]) => {
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
// The checker accepts both of these. The second returns nonsense at run time.
export const fine = patientFetch("https://example.test", 10);
export const wrong = patientFetch(10, "https://example.test");
