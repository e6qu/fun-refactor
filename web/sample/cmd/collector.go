package collector

import (
	"errors"
	"fmt"
	"sort"
)

// Reading is one measurement from one sensor.
type Reading struct {
	Sensor  string
	Celsius float64
	At      int64
}

// Limits bound what a plausible reading looks like.
type Limits struct {
	MinCelsius float64
	MaxCelsius float64
}

// DefaultLimits are what the collector uses when nothing else is configured.
func DefaultLimits() Limits {
	return Limits{MinCelsius: -80, MaxCelsius: 120}
}

var ErrNoSensor = errors.New("a reading with no sensor is not attributable")

// Validate reports why a reading cannot be stored, or nil.
func Validate(r Reading, limits Limits) error {
	if r.Sensor == "" {
		return ErrNoSensor
	}
	if r.Celsius < limits.MinCelsius {
		return fmt.Errorf("%v below the floor", r.Celsius)
	}
	if r.Celsius > limits.MaxCelsius {
		return fmt.Errorf("%v above the ceiling", r.Celsius)
	}
	return nil
}

// Averages returns the mean per sensor.
func Averages(readings []Reading) map[string]float64 {
	sums := map[string]float64{}
	counts := map[string]int{}
	for _, r := range readings {
		sums[r.Sensor] += r.Celsius
		counts[r.Sensor]++
	}
	means := map[string]float64{}
	for sensor, total := range sums {
		means[sensor] = total / float64(counts[sensor])
	}
	return means
}

// Rejects lists why each unusable reading was refused, in arrival order.
func Rejects(readings []Reading, limits Limits) []string {
	var out []string
	for _, r := range readings {
		if err := Validate(r, limits); err != nil {
			out = append(out, fmt.Sprintf("%s: %v", r.Sensor, err))
		}
	}
	return out
}

// Sensors lists every sensor seen, sorted.
func Sensors(readings []Reading) []string {
	seen := map[string]bool{}
	var names []string
	for _, r := range readings {
		if !seen[r.Sensor] {
			seen[r.Sensor] = true
			names = append(names, r.Sensor)
		}
	}
	sort.Strings(names)
	return names
}

// Fahrenheit is called from nowhere, which is the point.
func Fahrenheit(celsius float64) float64 {
	return celsius*9/5 + 32
}
