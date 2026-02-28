FROM rust:1.84-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release
RUN strip target/release/zenproxy

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/zenproxy .
COPY config.toml .
RUN mkdir -p data
EXPOSE 3000
CMD ["./zenproxy"]
