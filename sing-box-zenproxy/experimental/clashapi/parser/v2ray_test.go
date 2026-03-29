package parser

import (
	"encoding/json"
	"testing"
)

func outboundAuthFields(t *testing.T, uri string) (string, string) {
	t.Helper()

	proxy := ParseURI(uri)
	if proxy == nil {
		t.Fatalf("uri should parse: %s", uri)
	}

	var outbound map[string]any
	if err := json.Unmarshal(proxy.Outbound, &outbound); err != nil {
		t.Fatalf("failed to unmarshal outbound: %v", err)
	}

	username, _ := outbound["username"].(string)
	password, _ := outbound["password"].(string)
	return username, password
}

func TestParseSocksKeepsPlaintextUserinfoBehavior(t *testing.T) {
	username, password := outboundAuthFields(t, "socks5://ry:62132@100.99.99.1:20000#oracle-de-lite1-v4")

	if username != "ry" {
		t.Fatalf("expected username ry, got %q", username)
	}
	if password != "62132" {
		t.Fatalf("expected password 62132, got %q", password)
	}
}

func TestParseSocksDecodesPercentEncodedUserinfo(t *testing.T) {
	username, password := outboundAuthFields(t, "socks5://ry%3A62132@100.99.99.1:20000#oracle-de-lite1-v4")

	if username != "ry" {
		t.Fatalf("expected username ry, got %q", username)
	}
	if password != "62132" {
		t.Fatalf("expected password 62132, got %q", password)
	}
}

func TestParseSocksDecodesPercentEncodedBase64Userinfo(t *testing.T) {
	username, password := outboundAuthFields(t, "socks5://cnk6NjIxMzI%3D@100.99.99.1:20000#oracle-de-lite1-v4")

	if username != "ry" {
		t.Fatalf("expected username ry, got %q", username)
	}
	if password != "62132" {
		t.Fatalf("expected password 62132, got %q", password)
	}
}

func TestParseHTTPDecodesPercentEncodedBase64Userinfo(t *testing.T) {
	username, password := outboundAuthFields(t, "http://cnk6NjIxMzI%3D@100.99.99.1:20000")

	if username != "ry" {
		t.Fatalf("expected username ry, got %q", username)
	}
	if password != "62132" {
		t.Fatalf("expected password 62132, got %q", password)
	}
}

