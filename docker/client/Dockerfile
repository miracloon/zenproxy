# ============================================================
# Stage 1: Build sing-box-zenproxy (Go) — native cross-compilation
# ============================================================
FROM --platform=$BUILDPLATFORM golang:1.24-alpine AS builder

ARG TARGETOS
ARG TARGETARCH

COPY sing-box-zenproxy /go/src/github.com/sagernet/sing-box
WORKDIR /go/src/github.com/sagernet/sing-box

ENV CGO_ENABLED=0

RUN set -ex \
    && apk add --no-cache git build-base \
    && GOOS=${TARGETOS} GOARCH=${TARGETARCH} \
       go build -v -trimpath \
        -tags "with_gvisor,with_quic,with_dhcp,with_wireguard,with_utls,with_acme,with_clash_api,with_tailscale,with_ccm,with_ocm,badlinkname,tfogo_checklinkname0" \
        -o /go/bin/sing-box \
        -ldflags "-s -w -buildid=" \
        ./cmd/sing-box

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder /go/bin/sing-box /app/sing-box

WORKDIR /app

VOLUME ["/app/data"]

EXPOSE 9090

ENTRYPOINT ["/app/sing-box", "run", "-c", "/app/config.json"]
