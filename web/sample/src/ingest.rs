//! The ingest path: a reading arrives, is validated, and is handed to a sink.

use std::collections::HashMap;

pub struct Reading {
    pub sensor: String,
    pub celsius: f64,
    pub at: u64,
}

pub struct Limits {
    pub min_celsius: f64,
    pub max_celsius: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            min_celsius: -80.0,
            max_celsius: 120.0,
        }
    }
}

pub fn validate(reading: &Reading, limits: &Limits) -> Result<(), String> {
    if reading.sensor.is_empty() {
        return Err("a reading with no sensor is not attributable".into());
    }
    if reading.celsius < limits.min_celsius {
        return Err(format!("{} below the floor", reading.celsius));
    }
    if reading.celsius > limits.max_celsius {
        return Err(format!("{} above the ceiling", reading.celsius));
    }
    Ok(())
}

/// The mean of every reading a sensor has sent, keyed by sensor.
pub fn averages(readings: &[Reading]) -> HashMap<String, f64> {
    let mut sums: HashMap<String, (f64, usize)> = HashMap::new();
    for reading in readings {
        let entry = sums.entry(reading.sensor.clone()).or_insert((0.0, 0));
        entry.0 += reading.celsius;
        entry.1 += 1;
    }
    sums.into_iter()
        .map(|(sensor, (total, count))| (sensor, total / count as f64))
        .collect()
}

/// Readings outside the limits, in arrival order.
pub fn rejects(readings: &[Reading], limits: &Limits) -> Vec<String> {
    let mut out = Vec::new();
    for reading in readings {
        if let Err(why) = validate(reading, limits) {
            out.push(format!("{}: {}", reading.sensor, why));
        }
    }
    out
}

/// Nothing calls this. It is here so the dead-code report has something to find.
pub fn hottest(readings: &[Reading]) -> Option<&Reading> {
    readings.iter().max_by(|a, b| a.celsius.total_cmp(&b.celsius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_sensor_is_refused() {
        let reading = Reading {
            sensor: String::new(),
            celsius: 20.0,
            at: 0,
        };
        assert!(validate(&reading, &Limits::default()).is_err());
    }

    #[test]
    fn averages_are_per_sensor() {
        let readings = vec![
            Reading { sensor: "a".into(), celsius: 10.0, at: 0 },
            Reading { sensor: "a".into(), celsius: 20.0, at: 1 },
            Reading { sensor: "b".into(), celsius: 5.0, at: 2 },
        ];
        let means = averages(&readings);
        assert_eq!(means["a"], 15.0);
        assert_eq!(means["b"], 5.0);
    }
}
