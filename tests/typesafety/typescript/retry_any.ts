// expect: passes

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
export const fine = patientFetch("https://example.test", 10);
export const wrong = patientFetch(10, "https://example.test"); // accepted, and wrong
