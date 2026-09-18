package frgenerated

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http/httptest"
	"os"
	"testing"
)

func TestApplication(t *testing.T) {
	data, err := os.ReadFile("cases.json")
	if err != nil {
		t.Fatal(err)
	}
	var cases []struct {
		Method string `json:"method"`
		URL    string `json:"url"`
		Body   any    `json:"body"`
	}
	if err := json.Unmarshal(data, &cases); err != nil {
		t.Fatal(err)
	}
	handler := Handler()
	results := []any{}
	for _, test := range cases {
		response := httptest.NewRecorder()
		var requestBody []byte
		if test.Body != nil {
			requestBody, err = json.Marshal(test.Body)
			if err != nil {
				t.Fatal(err)
			}
		}
		request := httptest.NewRequest(test.Method, test.URL, bytes.NewReader(requestBody))
		if test.Body != nil {
			request.Header.Set("Content-Type", "application/json")
		}
		handler.ServeHTTP(response, request)
		var body any
		if err := json.Unmarshal(response.Body.Bytes(), &body); err != nil {
			t.Fatal(err)
		}
		results = append(results, map[string]any{"status": response.Code, "body": body})
	}
	encoded, err := json.Marshal(results)
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("FR_APPLICATION_RESULT=" + string(encoded))
}
