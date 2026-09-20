package sample
import ("encoding/json"; "net/http"; "strconv")
func show(w http.ResponseWriter, r *http.Request) {
    limit, err := strconv.Atoi(r.URL.Query().Get("limit"))
    if err != nil { w.WriteHeader(422); return }
    w.WriteHeader(201)
    json.NewEncoder(w).Encode(map[string]any{"id": r.PathValue("id"), "limit": limit})
}
func routes() http.Handler {
    mux := http.NewServeMux()
    mux.HandleFunc("GET /records/{id}", show)
    return mux
}
