package clashapi

import (
	"os"
	"testing"
	"time"
)

func waitForStoredProxyCount(t *testing.T, dataDir string, want int) []StoredProxy {
	t.Helper()

	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		storePath := dataDir + "/store.json"
		if _, err := os.Stat(storePath); err == nil {
			reloaded := NewProxyStore(dataDir, nil)
			proxies := reloaded.ListProxies()
			if len(proxies) == want {
				return proxies
			}
		}
		time.Sleep(10 * time.Millisecond)
	}

	t.Fatalf("timed out waiting for store.json to contain %d proxies", want)
	return nil
}

func TestProxyStore_RemoveBySourceOnlyRemovesMatchingEntries(t *testing.T) {
	dataDir := t.TempDir()
	store := NewProxyStore(dataDir, nil)

	store.AddProxy(StoredProxy{ID: "server-1", Name: "server-1", Source: "server"})
	store.AddProxy(StoredProxy{ID: "manual-1", Name: "manual-1", Source: "manual"})
	store.AddProxy(StoredProxy{ID: "sub-1", Name: "sub-1", Source: "subscription"})
	store.AddProxy(StoredProxy{ID: "server-2", Name: "server-2", Source: "server"})

	removed := store.RemoveBySource("server")
	if removed != 2 {
		t.Fatalf("expected to remove 2 server proxies, got %d", removed)
	}

	proxies := waitForStoredProxyCount(t, dataDir, 2)

	for _, proxy := range proxies {
		if proxy.Source == "server" {
			t.Fatalf("expected all server proxies to be removed, found %s", proxy.ID)
		}
	}
}

func TestProxyStore_RemoveBySourcePersistsRemainingEntries(t *testing.T) {
	dataDir := t.TempDir()
	store := NewProxyStore(dataDir, nil)

	store.AddProxy(StoredProxy{ID: "server-1", Name: "server-1", Source: "server"})
	store.AddProxy(StoredProxy{ID: "manual-1", Name: "manual-1", Source: "manual"})

	store.RemoveBySource("server")
	proxies := waitForStoredProxyCount(t, dataDir, 1)
	if proxies[0].Source != "manual" {
		t.Fatalf("expected manual proxy to remain, got %s", proxies[0].Source)
	}
}
