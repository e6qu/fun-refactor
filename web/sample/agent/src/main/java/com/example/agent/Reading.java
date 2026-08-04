package com.example.agent;

/** One sample from a sensor. The same shape as the Go and Rust `Reading`. */
public record Reading(String sensor, double value) {
    public boolean isWarm(double limit) {
        return value > limit;
    }
}
