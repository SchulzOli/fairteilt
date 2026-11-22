# Build context: repository root (docker compose sets it via `context: ..`).
FROM rust:1-trixie AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --profile release-server -p punkte-server \
 && cp target/release-server/punkte-server /punkte-server

FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 punkte
COPY --from=build /punkte-server /usr/local/bin/punkte-server
USER punkte
ENV BIND_ADDR=0.0.0.0:8080 \
    RUST_LOG=punkte_server=info,tower_http=info
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://localhost:8080/health || exit 1
CMD ["punkte-server"]
