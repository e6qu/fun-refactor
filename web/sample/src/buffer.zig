//! A fixed-size ring the collector writes readings into.

const std = @import("std");

pub const Reading = struct {
    sensor: []const u8,
    celsius: f64,
    at: u64,
};

pub const min_celsius: f64 = -80.0;
pub const max_celsius: f64 = 120.0;

pub fn validate(reading: Reading) ?[]const u8 {
    if (reading.sensor.len == 0) {
        return "a reading with no sensor is not attributable";
    }
    if (reading.celsius < min_celsius) {
        return "below the floor";
    }
    if (reading.celsius > max_celsius) {
        return "above the ceiling";
    }
    return null;
}

pub const Ring = struct {
    items: []Reading,
    head: usize = 0,
    len: usize = 0,

    pub fn init(items: []Reading) Ring {
        return Ring{ .items = items };
    }

    pub fn push(self: *Ring, reading: Reading) void {
        self.items[self.head] = reading;
        self.head = (self.head + 1) % self.items.len;
        if (self.len < self.items.len) {
            self.len += 1;
        }
    }

    pub fn mean(self: *const Ring) f64 {
        if (self.len == 0) {
            return 0;
        }
        var total: f64 = 0;
        var i: usize = 0;
        while (i < self.len) : (i += 1) {
            total += self.items[i].celsius;
        }
        return total / @as(f64, @floatFromInt(self.len));
    }
};

pub fn fahrenheit(celsius: f64) f64 {
    return celsius * 9.0 / 5.0 + 32.0;
}
