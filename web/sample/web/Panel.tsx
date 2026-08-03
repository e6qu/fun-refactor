import { useEffect, useState } from "react";
import { averages, fetchReadings, formatCelsius, type Reading } from "./dashboard";

interface PanelProps {
  base: string;
  showRejected: boolean;
}

function SensorRow({ sensor, mean }: { sensor: string; mean: number }) {
  return (
    <tr className="sensor-row">
      <td className="sensor-name">{sensor}</td>
      <td className="sensor-mean">{formatCelsius(mean)}</td>
    </tr>
  );
}

export function Panel({ base, showRejected }: PanelProps) {
  const [readings, setReadings] = useState<Reading[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchReadings(base).then(setReadings).catch((e) => setError(String(e)));
  }, [base]);

  if (error !== null) {
    return <p className="panel-error">{error}</p>;
  }

  const means = averages(readings);
  return (
    <table className="panel">
      <tbody>
        {Object.entries(means).map(([sensor, mean]) => (
          <SensorRow key={sensor} sensor={sensor} mean={mean} />
        ))}
      </tbody>
    </table>
  );
}
