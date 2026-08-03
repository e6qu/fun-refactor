/** The dashboard's data layer: fetch, shape, and format what the collector serves. */

export interface Reading {
  sensor: string;
  celsius: number;
  at: number;
}

export interface Limits {
  minCelsius: number;
  maxCelsius: number;
}

export const defaultLimits: Limits = { minCelsius: -80, maxCelsius: 120 };

export function validate(reading: Reading, limits: Limits): string | null {
  if (!reading.sensor) {
    return "a reading with no sensor is not attributable";
  }
  if (reading.celsius < limits.minCelsius) {
    return `${reading.celsius} below the floor`;
  }
  if (reading.celsius > limits.maxCelsius) {
    return `${reading.celsius} above the ceiling`;
  }
  return null;
}

export function averages(readings: Reading[]): Record<string, number> {
  const sums: Record<string, number> = {};
  const counts: Record<string, number> = {};
  for (const reading of readings) {
    sums[reading.sensor] = (sums[reading.sensor] ?? 0) + reading.celsius;
    counts[reading.sensor] = (counts[reading.sensor] ?? 0) + 1;
  }
  const means: Record<string, number> = {};
  for (const sensor of Object.keys(sums)) {
    means[sensor] = sums[sensor] / counts[sensor];
  }
  return means;
}

export function rejects(readings: Reading[], limits: Limits): string[] {
  const out: string[] = [];
  for (const reading of readings) {
    const why = validate(reading, limits);
    if (why !== null) {
      out.push(`${reading.sensor}: ${why}`);
    }
  }
  return out;
}

export function formatCelsius(value: number): string {
  return `${value.toFixed(1)}°C`;
}

export async function fetchReadings(base: string): Promise<Reading[]> {
  const response = await fetch(`${base}/readings`);
  if (!response.ok) {
    throw new Error(`the collector returned ${response.status}`);
  }
  return response.json();
}

/** Exported, used nowhere in this workspace: a public API, not dead code. */
export function fahrenheit(celsius: number): number {
  return (celsius * 9) / 5 + 32;
}
