//! Wires the ingest path to a sink and prints what it rejected.

mod convert;
mod ingest;

use convert::{for_display, unit};
use ingest::{averages, rejects, Limits, Reading};

fn sample() -> Vec<Reading> {
    vec![
        Reading { sensor: "roof".into(), celsius: 21.5, at: 1 },
        Reading { sensor: "roof".into(), celsius: 22.0, at: 2 },
        Reading { sensor: "cellar".into(), celsius: 9.0, at: 3 },
        Reading { sensor: "cellar".into(), celsius: 900.0, at: 4 },
    ]
}

fn report(readings: &[Reading], limits: &Limits) {
    for (sensor, mean) in averages(readings) {
        println!("{sensor}: {:.1}{}", for_display(mean), unit());
    }
    for line in rejects(readings, limits) {
        eprintln!("rejected {line}");
    }
}

fn main() {
    let limits = Limits::default();
    let readings = sample();
    if !readings.is_empty() {
        report(&readings, &limits);
    }
}
