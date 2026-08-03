package collector

import (
	"encoding/json"
	"net/http"
)

// Server answers the two questions the dashboard asks.
type Server struct {
	Readings []Reading
	Limits   Limits
	Verbose  bool
}

func NewServer(readings []Reading) *Server {
	return &Server{Readings: readings, Limits: DefaultLimits()}
}

func (s *Server) handleAverages(w http.ResponseWriter, r *http.Request) {
	means := Averages(s.Readings)
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(means); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

func (s *Server) handleRejects(w http.ResponseWriter, r *http.Request) {
	refused := Rejects(s.Readings, s.Limits)
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(refused); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

// Routes registers every handler. Note the two bodies above: identical but for one
// call, which is what the copy-paste detector is meant to notice.
func (s *Server) Routes(mux *http.ServeMux) {
	mux.HandleFunc("/averages", s.handleAverages)
	mux.HandleFunc("/rejects", s.handleRejects)
	mux.HandleFunc("/sensors", func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(Sensors(s.Readings))
	})
}
