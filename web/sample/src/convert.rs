//! Unit conversion, kept apart from the ingest path so `fr move` has somewhere to
//! move things to.

/// Whether readings are reported in the sensor's own scale or converted first.
///
/// Shipped on everywhere in 2024; the switch is what is left over. Retiring it is
/// what `fr remove-flag REPORT_IN_CELSIUS` is for.
pub const REPORT_IN_CELSIUS: bool = true;

pub fn fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

pub fn celsius(fahrenheit: f64) -> f64 {
    (fahrenheit - 32.0) * 5.0 / 9.0
}

/// The value to display, honouring the flag.
pub fn for_display(reading_celsius: f64) -> f64 {
    if REPORT_IN_CELSIUS {
        reading_celsius
    } else {
        fahrenheit(reading_celsius)
    }
}

/// The unit label that goes with it.
pub fn unit() -> &'static str {
    if !REPORT_IN_CELSIUS {
        "F"
    } else {
        "C"
    }
}
