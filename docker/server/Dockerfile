# ============================================================
# Stage 0: xx cross-compilation helpers
# ============================================================
FROM --platform=$BUILDPLATFORM tonistiigi/xx AS xx

# ============================================================
# Stage 1: Build sing-box (Go) — native cross-compilation
# ============================================================
FROM --platform=$BUILDPLATFORM golang:1.24-alpine AS builder-go

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
# Stage 2: Build zenproxy (Rust) — native cross-compilation
#
# Uses xx toolkit with clang as the cross-compiler/linker.
# All compilation runs natively on $BUILDPLATFORM (amd64).
# ============================================================
FROM --platform=$BUILDPLATFORM rust:1.86-alpine AS builder-rust

COPY --from=xx / /
ARG TARGETPLATFORM

# Install cross-compilation toolchain via xx
RUN apk add --no-cache clang lld musl-dev
RUN xx-apk add --no-cache musl-dev

# Add Rust target and configure cross-linker + C compiler
RUN RUST_TARGET="$(xx-info march)-unknown-linux-musl" \
    && rustup target add ${RUST_TARGET} \
    && mkdir -p /build/.cargo \
    && printf "[target.aarch64-unknown-linux-musl]\nlinker = \"/usr/bin/xx-clang\"\n" > /build/.cargo/config.toml

# Set CC for cc-rs crate (used by rusqlite/SQLite C compilation)
ENV CC_aarch64_unknown_linux_musl=/usr/bin/xx-clang
ENV CFLAGS_aarch64_unknown_linux_musl="--target=aarch64-alpine-linux-musl --sysroot=/aarch64-alpine-linux-musl"

# Dependency caching: build deps first with stub
COPY Cargo.toml Cargo.lock /build/
WORKDIR /build
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && RUST_TARGET="$(xx-info march)-unknown-linux-musl" \
    && cargo build --release --target ${RUST_TARGET} 2>/dev/null || true \
    && rm -rf src

# Build the actual project
COPY src /build/src
COPY README.md /build/README.md

RUN RUST_TARGET="$(xx-info march)-unknown-linux-musl" \
    && touch src/main.rs \
    && cargo build --release --target ${RUST_TARGET} \
    && cp target/${RUST_TARGET}/release/zenproxy /build/zenproxy-out

# ============================================================
# Stage 3: Runtime
# ============================================================
FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder-go /go/bin/sing-box /app/sing-box
COPY --from=builder-rust /build/zenproxy-out /app/zenproxy

WORKDIR /app

VOLUME ["/app/data"]

EXPOSE 3000 9090

ENTRYPOINT ["/app/zenproxy"]
