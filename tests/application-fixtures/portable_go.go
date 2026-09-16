package sample

import (
    "encoding/json"
    "net/http"
)

func showGo(w http.ResponseWriter, r *http.Request) {
    w.WriteHeader(201)
    json.NewEncoder(w).Encode(map[string]any{"id": r.PathValue("id"), "framework": "portable"})
}

func routes() http.Handler {
    mux := http.NewServeMux()
    mux.HandleFunc("GET /go/{id}", showGo)
    return mux
}
