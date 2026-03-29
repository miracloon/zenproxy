package clashapi

import (
	"os"
	"strconv"
)

type startupFetchConfig struct {
	Enabled        bool
	Server         string
	APIKey         string
	All            bool
	Count          int
	Country        string
	Type           string
	ChatGPT        *bool
	AutoBind       bool
	SyncRemotePort *bool
}

func loadStartupFetchConfig(getenv func(string) string) startupFetchConfig {
	cfg := startupFetchConfig{
		Enabled:  parseEnvBool(getenv, "REMOTE_FETCH_ENABLED", true),
		Server:   getenv("REMOTE_FETCH_SERVER"),
		APIKey:   getenv("REMOTE_FETCH_API_KEY"),
		All:      parseEnvBool(getenv, "REMOTE_FETCH_ALL", false),
		Count:    parseEnvInt(getenv, "REMOTE_FETCH_COUNT", 10),
		Country:  getenv("REMOTE_FETCH_COUNTRY"),
		Type:     getenv("REMOTE_FETCH_TYPE"),
		ChatGPT:  parseOptionalEnvBool(getenv, "REMOTE_FETCH_CHATGPT"),
		AutoBind: parseEnvBool(getenv, "REMOTE_FETCH_AUTO_BIND", true),
	}

	cfg.SyncRemotePort = parseOptionalEnvBool(getenv, "REMOTE_FETCH_SYNC_REMOTE_PORT")
	if cfg.All {
		cfg.Count = 0
	}

	return cfg
}

func (c startupFetchConfig) ShouldRun() bool {
	return c.Enabled && c.Server != "" && c.APIKey != ""
}

func (c startupFetchConfig) toRemoteFetchRequest() remoteFetchRequest {
	return remoteFetchRequest{
		Server:         c.Server,
		APIKey:         c.APIKey,
		All:            c.All,
		Count:          c.Count,
		Country:        c.Country,
		ChatGPT:        c.ChatGPT,
		Type:           c.Type,
		AutoBind:       c.AutoBind,
		SyncRemotePort: c.SyncRemotePort,
	}
}

func parseEnvBool(getenv func(string) string, key string, defaultValue bool) bool {
	raw := getenv(key)
	if raw == "" {
		return defaultValue
	}
	parsed, err := strconv.ParseBool(raw)
	if err != nil {
		return defaultValue
	}
	return parsed
}

func parseOptionalEnvBool(getenv func(string) string, key string) *bool {
	raw := getenv(key)
	if raw == "" {
		return nil
	}
	parsed, err := strconv.ParseBool(raw)
	if err != nil {
		return nil
	}
	return &parsed
}

func parseEnvInt(getenv func(string) string, key string, defaultValue int) int {
	raw := getenv(key)
	if raw == "" {
		return defaultValue
	}
	parsed, err := strconv.Atoi(raw)
	if err != nil {
		return defaultValue
	}
	return parsed
}

func (bm *BindingManager) runStartupFetch() {
	cfg := loadStartupFetchConfig(func(key string) string {
		return os.Getenv(key)
	})

	if !cfg.Enabled {
		bm.logger.Info("startup remote fetch disabled")
		return
	}
	if !cfg.ShouldRun() {
		bm.logger.Warn("startup remote fetch skipped: REMOTE_FETCH_SERVER or REMOTE_FETCH_API_KEY is missing")
		return
	}

	removed := bm.store.RemoveBySource("server")
	if removed > 0 {
		bm.logger.Info("startup remote fetch cleared ", removed, " existing server proxies")
	}

	result, _, err := bm.executeRemoteFetch(cfg.toRemoteFetchRequest())
	if err != nil {
		bm.logger.Error("startup remote fetch failed: ", err)
		return
	}

	bm.logger.Info("startup remote fetch completed: added=", result.Added, " bound=", result.Bound)
	if len(result.SyncErrors) > 0 {
		bm.logger.Warn("startup remote fetch sync errors: ", len(result.SyncErrors))
	}
}
