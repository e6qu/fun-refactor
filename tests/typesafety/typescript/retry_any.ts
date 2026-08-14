// expect: passes

class ConnectionLost extends Error {}

function retry(times: number, operation: (...args: any[]) => any): (...args: any[]) => any {
  return (...args: any[]) => {
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
export const fine = patientFetch("https://example.test", 10);
export const wrong = patientFetch(10, "https://example.test");
