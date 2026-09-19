package frgenerated

import (
	"encoding/json"
	"fmt"
	"net/http/httptest"
	"testing"
)

func TestBehavior(t *testing.T) {
	handler := Handler()
	results := []any{}
	for _, url := range []string{"/guarded", "/guarded?token=present"} {
		response := httptest.NewRecorder()
		request := httptest.NewRequest("GET", url, nil)
		handler.ServeHTTP(response, request)
		var body any
		if err := json.Unmarshal(response.Body.Bytes(), &body); err != nil {
			t.Fatal(err)
		}
		results = append(results, map[string]any{
			"status": response.Code,
			"body":   body,
			"order":  response.Header().Values("x-fr-order"),
		})
	}
	encoded, err := json.Marshal(results)
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("FR_APPLICATION_RESULT=" + string(encoded))
}
