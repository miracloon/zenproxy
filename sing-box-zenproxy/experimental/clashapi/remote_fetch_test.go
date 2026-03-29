package clashapi

import "testing"

func TestRemoteFetch_NormalizeDefaultsCountToTen(t *testing.T) {
	req := normalizeRemoteFetchRequest(remoteFetchRequest{
		Server: "https://example.com",
		APIKey: "k",
	})

	if req.Count != 10 {
		t.Fatalf("expected default count 10, got %d", req.Count)
	}
	if req.All {
		t.Fatalf("expected all=false by default")
	}
}

func TestRemoteFetch_NormalizeAllIgnoresCount(t *testing.T) {
	req := normalizeRemoteFetchRequest(remoteFetchRequest{
		Server: "https://example.com",
		APIKey: "k",
		All:    true,
		Count:  123,
	})

	if !req.All {
		t.Fatalf("expected all=true to be preserved")
	}
	if req.Count != 0 {
		t.Fatalf("expected count to be ignored in all mode, got %d", req.Count)
	}
}

func TestRemoteFetch_BuildURLUsesAllWithoutCount(t *testing.T) {
	req := normalizeRemoteFetchRequest(remoteFetchRequest{
		Server:  "https://example.com",
		APIKey:  "secret",
		All:     true,
		Country: "US",
		Type:    "vmess",
	})

	fetchURL := buildServerFetchURL(req)

	expected := "https://example.com/api/client/fetch?all=true&api_key=secret&country=US&type=vmess"
	if fetchURL != expected {
		t.Fatalf("unexpected fetch url: %s", fetchURL)
	}
}

func TestRemoteFetch_BuildURLKeepsSingleValueFiltersAndCount(t *testing.T) {
	req := normalizeRemoteFetchRequest(remoteFetchRequest{
		Server:  "https://example.com",
		APIKey:  "secret",
		Count:   25,
		Country: "US,JP",
		Type:    "vmess,vless",
	})

	fetchURL := buildServerFetchURL(req)

	expected := "https://example.com/api/client/fetch?api_key=secret&count=25&country=US%2CJP&type=vmess%2Cvless"
	if fetchURL != expected {
		t.Fatalf("unexpected fetch url: %s", fetchURL)
	}
}

func TestRemoteFetch_ResolveSyncRemotePortPrefersRequestOverride(t *testing.T) {
	falseValue := false
	trueValue := true

	if !resolveSyncRemotePort(remoteFetchRequest{SyncRemotePort: &trueValue}, false) {
		t.Fatalf("expected request override true to win over env false")
	}
	if resolveSyncRemotePort(remoteFetchRequest{SyncRemotePort: &falseValue}, true) {
		t.Fatalf("expected request override false to win over env true")
	}
	if !resolveSyncRemotePort(remoteFetchRequest{}, true) {
		t.Fatalf("expected env default to be used when request override is absent")
	}
}

func TestRemoteFetch_ServerProxiesToStoredProxiesMapsLocalPortToRemotePort(t *testing.T) {
	port := uint16(12001)
	proxies := serverProxiesToStoredProxies([]serverProxy{
		{
			ID:        "proxy-1",
			Name:      "proxy-1",
			Type:      "vmess",
			Server:    "example.com",
			Port:      443,
			LocalPort: &port,
			Outbound:  []byte(`{"type":"vmess"}`),
		},
	})

	if len(proxies) != 1 {
		t.Fatalf("expected 1 stored proxy, got %d", len(proxies))
	}
	if proxies[0].RemotePort != port {
		t.Fatalf("expected remote port %d, got %d", port, proxies[0].RemotePort)
	}
	if proxies[0].Source != "server" {
		t.Fatalf("expected source=server, got %s", proxies[0].Source)
	}
}
