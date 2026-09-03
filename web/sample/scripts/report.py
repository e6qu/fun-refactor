"""Offline reporting over a day of readings."""

from collections import defaultdict
import json
import os

MIN_CELSIUS = -80.0
MAX_CELSIUS = 120.0


def validate(reading, min_celsius=MIN_CELSIUS, max_celsius=MAX_CELSIUS):
    """Return why a reading is unusable, or None."""
    if not reading.get("sensor"):
        return "a reading with no sensor is not attributable"
    celsius = reading["celsius"]
    if celsius < min_celsius:
        return f"{celsius} below the floor"
    if celsius > max_celsius:
        return f"{celsius} above the ceiling"
    return None


def averages(readings):
    """Mean per sensor."""
    sums = defaultdict(float)
    counts = defaultdict(int)
    for reading in readings:
        sums[reading["sensor"]] += reading["celsius"]
        counts[reading["sensor"]] += 1
    return {sensor: total / counts[sensor] for sensor, total in sums.items()}


def rejects(readings):
    """Every reading that could not be stored, and why."""
    out = []
    for reading in readings:
        why = validate(reading)
        if why is not None:
            out.append(f"{reading.get('sensor', '?')}: {why}")
    return out


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def main():
    path = os.environ.get("READINGS", "readings.json")
    readings = load(path)
    for sensor, mean in sorted(averages(readings).items()):
        print(f"{sensor}: {mean:.1f}C")
    for line in rejects(readings):
        print(f"rejected {line}")


if __name__ == "__main__":
    main()
