package frgenerated

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestServiceCalls(t *testing.T) {
	server := httptest.NewServer(Handler())
	defer server.Close()
	results := []any{}
	for _, path := range []string{"/records", "/feed"} {
		response, err := http.Get(server.URL + path)
		if err != nil {
			t.Fatal(err)
		}
		var body any
		if err := json.NewDecoder(response.Body).Decode(&body); err != nil {
			t.Fatal(err)
		}
		_ = response.Body.Close()
		results = append(results, map[string]any{"status": response.StatusCode, "body": body})
	}
	encoded, err := json.Marshal(results)
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("FR_APPLICATION_RESULT=" + string(encoded))
}
