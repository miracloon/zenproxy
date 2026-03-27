package clashapi

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
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

	if req.Server == "" || req.APIKey == "" {
		render.Status(r, http.StatusBadRequest)
		render.JSON(w, r, newError("server and api_key are required"))
		return
	}
	if req.Count <= 0 {
		req.Count = 10
	}

	// Build server URL
	params := url.Values{}
	params.Set("api_key", req.APIKey)
	params.Set("count", fmt.Sprintf("%d", req.Count))
	if req.Country != "" {
		params.Set("country", req.Country)
	}
	if req.ChatGPT != nil && *req.ChatGPT {
		params.Set("chatgpt", "true")
	}
	if req.Type != "" {
		params.Set("type", req.Type)
	}

	fetchURL := fmt.Sprintf("%s/api/client/fetch?%s", req.Server, params.Encode())

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Get(fetchURL)
	if err != nil {
		render.Status(r, http.StatusBadGateway)
		render.JSON(w, r, newError("failed to fetch from server: "+err.Error()))
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		render.Status(r, resp.StatusCode)
		render.JSON(w, r, newError(fmt.Sprintf("server returned %d: %s", resp.StatusCode, string(respBody))))
		return
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		render.Status(r, http.StatusBadGateway)
		render.JSON(w, r, newError("failed to read server response: "+err.Error()))
		return
	}

	var serverResp serverFetchResponse
	if err := json.Unmarshal(respBody, &serverResp); err != nil {
		render.Status(r, http.StatusBadGateway)
		render.JSON(w, r, newError("failed to parse server response: "+err.Error()))
		return
	}

	// Store proxies (with remote port if available)
	proxies := make([]StoredProxy, 0, len(serverResp.Proxies))
	for _, sp := range serverResp.Proxies {
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

	added := bm.store.AddProxies(proxies)

	// Determine sync mode: request param > env var > false
	useSyncPort := bm.syncRemotePort
	if req.SyncRemotePort != nil {
		useSyncPort = *req.SyncRemotePort
	}

	// Auto-bind if requested
	bindCount := 0
	var syncErrors []map[string]string
	if req.AutoBind {
		for _, p := range added {
			if useSyncPort {
				// Sync mode: use remote port, no fallback
				if p.RemotePort == 0 {
					syncErrors = append(syncErrors, map[string]string{
						"proxy_id": p.ID,
						"name":     p.Name,
						"error":    "remote proxy has no local_port",
					})
					bm.logger.Warn("sync-port: proxy ", p.Name, " has no remote port, skipping")
					continue
				}
				if _, err := bm.createBindingDirect(p, p.RemotePort); err != nil {
					syncErrors = append(syncErrors, map[string]string{
						"proxy_id":    p.ID,
						"name":        p.Name,
						"remote_port": fmt.Sprintf("%d", p.RemotePort),
						"error":       err.Error(),
					})
					bm.logger.Warn("sync-port failed for ", p.Name, " port ", p.RemotePort, ": ", err)
				} else {
					bindCount++
				}
			} else {
				// Normal mode: auto-allocate from PortPool
				if bm.portPool == nil {
					bm.logger.Warn("auto-bind failed for ", p.Name, ": port pool not initialized")
					continue
				}
				if _, err := bm.createBindingForProxy(p); err != nil {
					bm.logger.Warn("auto-bind failed for ", p.Name, ": ", err)
				} else {
					bindCount++
				}
			}
		}
	}

	bm.logger.Info("fetched ", len(added), " proxies from server")
	result := render.M{
		"added":   len(added),
		"message": fmt.Sprintf("Fetched %d proxies from server", len(added)),
	}
	if req.AutoBind {
		result["bound"] = bindCount
		if len(syncErrors) > 0 {
			result["sync_errors"] = syncErrors
		}
	}
	render.JSON(w, r, result)
}
