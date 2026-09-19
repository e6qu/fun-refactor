package frgenerated

import (
	"errors"
	"net/http"
)

func mark(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Add("x-fr-order", "mark")
		next.ServeHTTP(w, r)
	})
}

func audit(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Add("x-fr-order", "audit")
		next.ServeHTTP(w, r)
	})
}

func load_session(r *http.Request) error {
	if r.URL.Query().Get("token") != "present" {
		return errors.New("denied")
	}
	return nil
}
