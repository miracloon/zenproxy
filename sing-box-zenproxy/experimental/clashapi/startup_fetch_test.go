package clashapi

import "testing"

func TestStartupFetch_DefaultsAreApplied(t *testing.T) {
	cfg := loadStartupFetchConfig(func(string) string {
		return ""
	})

	if !cfg.Enabled {
		t.Fatalf("expected startup fetch enabled by default")
	}
	if cfg.All {
		t.Fatalf("expected all=false by default")
	}
	if cfg.Count != 10 {
		t.Fatalf("expected default count 10, got %d", cfg.Count)
	}
	if !cfg.AutoBind {
		t.Fatalf("expected auto_bind=true by default")
	}
	if cfg.SyncRemotePort != nil {
		t.Fatalf("expected sync_remote_port to stay nil when env is unset")
	}
}

func TestStartupFetch_MissingServerOrAPIKeySkipsRun(t *testing.T) {
	cfg := loadStartupFetchConfig(func(key string) string {
		switch key {
		case "REMOTE_FETCH_ENABLED":
			return "true"
		case "REMOTE_FETCH_SERVER":
			return "https://example.com"
		default:
			return ""
		}
	})

	if cfg.ShouldRun() {
		t.Fatalf("expected startup fetch to skip when api key is missing")
	}
}

func TestStartupFetch_AllIgnoresCount(t *testing.T) {
	cfg := loadStartupFetchConfig(func(key string) string {
		switch key {
		case "REMOTE_FETCH_ENABLED":
			return "true"
		case "REMOTE_FETCH_SERVER":
			return "https://example.com"
		case "REMOTE_FETCH_API_KEY":
			return "secret"
		case "REMOTE_FETCH_ALL":
			return "true"
		case "REMOTE_FETCH_COUNT":
			return "999"
		default:
			return ""
		}
	})

	if !cfg.All {
		t.Fatalf("expected all=true")
	}
	if cfg.Count != 0 {
		t.Fatalf("expected count ignored in all mode, got %d", cfg.Count)
	}
	if !cfg.ShouldRun() {
		t.Fatalf("expected config to be runnable when required fields are present")
	}
}

func TestStartupFetch_RequestSyncRemotePortOverrideIsOptional(t *testing.T) {
	cfg := loadStartupFetchConfig(func(key string) string {
		switch key {
		case "REMOTE_FETCH_ENABLED":
			return "true"
		case "REMOTE_FETCH_SERVER":
			return "https://example.com"
		case "REMOTE_FETCH_API_KEY":
			return "secret"
		case "REMOTE_FETCH_SYNC_REMOTE_PORT":
			return "true"
		default:
			return ""
		}
	})

	if cfg.SyncRemotePort == nil || !*cfg.SyncRemotePort {
		t.Fatalf("expected sync_remote_port override to be set")
	}
}
