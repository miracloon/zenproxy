package clashapi

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/render"
)

func fetchRouter(bm *BindingManager) http.Handler {
	r := chi.NewRouter()
	r.Post("/", bm.remoteFetch)
	return r
}

type remoteFetchRequest struct {
	Server         string `json:"server"`
	APIKey         string `json:"api_key"`
	All            bool   `json:"all"`
	Count          int    `json:"count"`
	Country        string `json:"country"`
	ChatGPT        *bool  `json:"chatgpt"`
	Type           string `json:"type"`
	AutoBind       bool   `json:"auto_bind"`
	SyncRemotePort *bool  `json:"sync_remote_port,omitempty"`
}

type serverProxy struct {
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Type      string          `json:"type"`
	Server    string          `json:"server"`
	Port      uint16          `json:"port"`
	LocalPort *uint16         `json:"local_port,omitempty"`
	Outbound  json.RawMessage `json:"outbound"`
	Quality   json.RawMessage `json:"quality,omitempty"`
}

type serverFetchResponse struct {
	Proxies []serverProxy `json:"proxies"`
	Count   int           `json:"count"`
}

type remoteFetchResult struct {
	Added      int
	Bound      int
	SyncErrors []map[string]string
}

func normalizeRemoteFetchRequest(req remoteFetchRequest) remoteFetchRequest {
	if req.All {
		req.Count = 0
		return req
	}
	if req.Count <= 0 {
		req.Count = 10
	}
	return req
}

func buildServerFetchURL(req remoteFetchRequest) string {
	params := url.Values{}
	params.Set("api_key", req.APIKey)
	if req.All {
		params.Set("all", "true")
	} else {
		params.Set("count", fmt.Sprintf("%d", req.Count))
	}
	if req.Country != "" {
		params.Set("country", req.Country)
	}
	if req.ChatGPT != nil && *req.ChatGPT {
		params.Set("chatgpt", "true")
	}
	if req.Type != "" {
		params.Set("type", req.Type)
	}

	serverURL := strings.TrimRight(req.Server, "/")
	return fmt.Sprintf("%s/api/client/fetch?%s", serverURL, params.Encode())
}

func resolveSyncRemotePort(req remoteFetchRequest, defaultSync bool) bool {
	if req.SyncRemotePort != nil {
		return *req.SyncRemotePort
	}
	return defaultSync
}

func serverProxiesToStoredProxies(serverProxies []serverProxy) []StoredProxy {
	proxies := make([]StoredProxy, 0, len(serverProxies))
	for _, sp := range serverProxies {
		p := StoredProxy{
			ID:       sp.ID,
			Name:     sp.Name,
			Type:     sp.Type,
			Server:   sp.Server,
			Port:     sp.Port,
			Outbound: sp.Outbound,
			Source:   "server",
		}
		if sp.LocalPort != nil {
			p.RemotePort = *sp.LocalPort
		}
		proxies = append(proxies, p)
	}
	return proxies
}

func (bm *BindingManager) executeRemoteFetch(req remoteFetchRequest) (remoteFetchResult, int, error) {
	req = normalizeRemoteFetchRequest(req)
	if req.Server == "" || req.APIKey == "" {
		return remoteFetchResult{}, http.StatusBadRequest, fmt.Errorf("server and api_key are required")
	}

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Get(buildServerFetchURL(req))
	if err != nil {
		return remoteFetchResult{}, http.StatusBadGateway, fmt.Errorf("failed to fetch from server: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		return remoteFetchResult{}, resp.StatusCode, fmt.Errorf("server returned %d: %s", resp.StatusCode, string(respBody))
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return remoteFetchResult{}, http.StatusBadGateway, fmt.Errorf("failed to read server response: %w", err)
	}

	var serverResp serverFetchResponse
	if err := json.Unmarshal(respBody, &serverResp); err != nil {
		return remoteFetchResult{}, http.StatusBadGateway, fmt.Errorf("failed to parse server response: %w", err)
	}

	added := bm.store.AddProxies(serverProxiesToStoredProxies(serverResp.Proxies))
	useSyncPort := resolveSyncRemotePort(req, bm.syncRemotePort)

	result := remoteFetchResult{Added: len(added)}
	if !req.AutoBind {
		return result, http.StatusOK, nil
	}

	for _, p := range added {
		if useSyncPort {
			if p.RemotePort == 0 {
				result.SyncErrors = append(result.SyncErrors, map[string]string{
					"proxy_id": p.ID,
					"name":     p.Name,
					"error":    "remote proxy has no local_port",
				})
				bm.logger.Warn("sync-port: proxy ", p.Name, " has no remote port, skipping")
				continue
			}
			if _, err := bm.createBindingDirect(p, p.RemotePort); err != nil {
				result.SyncErrors = append(result.SyncErrors, map[string]string{
					"proxy_id":    p.ID,
					"name":        p.Name,
					"remote_port": fmt.Sprintf("%d", p.RemotePort),
					"error":       err.Error(),
				})
				bm.logger.Warn("sync-port failed for ", p.Name, " port ", p.RemotePort, ": ", err)
				continue
			}
			result.Bound++
			continue
		}

		if bm.portPool == nil {
			bm.logger.Warn("auto-bind failed for ", p.Name, ": port pool not initialized")
			continue
		}
		if _, err := bm.createBindingForProxy(p); err != nil {
			bm.logger.Warn("auto-bind failed for ", p.Name, ": ", err)
			continue
		}
		result.Bound++
	}

	return result, http.StatusOK, nil
}

func (bm *BindingManager) remoteFetch(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(r.Body)
	if err != nil {
		render.Status(r, http.StatusBadRequest)
		render.JSON(w, r, newError("failed to read body"))
		return
	}

	var req remoteFetchRequest
	if err := json.Unmarshal(body, &req); err != nil {
		render.Status(r, http.StatusBadRequest)
		render.JSON(w, r, newError("invalid JSON: "+err.Error()))
		return
	}

	fetchResult, status, err := bm.executeRemoteFetch(req)
	if err != nil {
		render.Status(r, status)
		render.JSON(w, r, newError(err.Error()))
		return
	}

	bm.logger.Info("fetched ", fetchResult.Added, " proxies from server")
	result := render.M{
		"added":   fetchResult.Added,
		"message": fmt.Sprintf("Fetched %d proxies from server", fetchResult.Added),
	}
	if req.AutoBind {
		result["bound"] = fetchResult.Bound
		if len(fetchResult.SyncErrors) > 0 {
			result["sync_errors"] = fetchResult.SyncErrors
		}
	}
	render.JSON(w, r, result)
}
