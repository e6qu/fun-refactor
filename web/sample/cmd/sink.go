package collector

import (
	"fmt"
	"sync"
)

// Sink is where a validated reading goes. Three things implement it, which is what
// makes "go to definition" on a Sink call an interesting question.
type Sink interface {
	Store(r Reading) error
	Flush() error
}

// MemorySink keeps everything in a slice. Used by the tests and the demo.
type MemorySink struct {
	mu    sync.Mutex
	items []Reading
}

func (s *MemorySink) Store(r Reading) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.items = append(s.items, r)
	return nil
}

func (s *MemorySink) Flush() error {
	return nil
}

// StdoutSink prints. Used when there is nowhere to write.
type StdoutSink struct {
	Prefix string
}

func (s *StdoutSink) Store(r Reading) error {
	fmt.Printf("%s%s %.1f\n", s.Prefix, r.Sensor, r.Celsius)
	return nil
}

func (s *StdoutSink) Flush() error {
	return nil
}

// BatchSink buffers and writes in groups.
type BatchSink struct {
	Inner Sink
	Size  int

	pending []Reading
}

func (s *BatchSink) Store(r Reading) error {
	s.pending = append(s.pending, r)
	if len(s.pending) < s.Size {
		return nil
	}
	return s.Flush()
}

func (s *BatchSink) Flush() error {
	for _, r := range s.pending {
		if err := s.Inner.Store(r); err != nil {
			return err
		}
	}
	s.pending = nil
	return s.Inner.Flush()
}

// Ingest validates each reading and hands the survivors to the sink.
func Ingest(readings []Reading, limits Limits, sink Sink) ([]string, error) {
	var refused []string
	for _, r := range readings {
		if err := Validate(r, limits); err != nil {
			refused = append(refused, fmt.Sprintf("%s: %v", r.Sensor, err))
			continue
		}
		if err := sink.Store(r); err != nil {
			return refused, err
		}
	}
	return refused, sink.Flush()
}
