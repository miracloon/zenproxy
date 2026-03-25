# ============================================================
# Stage 1: Build sing-box (Go)
# ============================================================
FROM golang:1.24-alpine AS builder-go

COPY sing-box-zenproxy /go/src/github.com/sagernet/sing-box
WORKDIR /go/src/github.com/sagernet/sing-box

ENV CGO_ENABLED=0

RUN set -ex \
    && apk add --no-cache git build-base \
    && go build -v -trimpath \
        -tags "with_clash_api" \
        -o /go/bin/sing-box \
        -ldflags "-s -w -buildid=" \
        ./cmd/sing-box

# ============================================================
# Stage 2: Build zenproxy (Rust)
# ============================================================
FROM rust:1.86-alpine AS builder-rust

RUN apk add --no-cache musl-dev

COPY Cargo.toml Cargo.lock /build/
COPY src /build/src
# Required by include_str! macros at compile time
COPY README.md /build/README.md
COPY docker/server/config/config.toml /build/config.toml

WORKDIR /build
RUN cargo build --release

# ============================================================
# Stage 3: Runtime
# ============================================================
FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder-go /go/bin/sing-box /app/sing-box
COPY --from=builder-rust /build/target/release/zenproxy /app/zenproxy

WORKDIR /app

VOLUME ["/app/data"]

EXPOSE 3000 9090

ENTRYPOINT ["/app/zenproxy"]
