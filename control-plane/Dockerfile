# syntax=docker/dockerfile:1
# Build context must be the repository root (see docker-compose.yml) so this
# can read the Cargo workspace rooted at control-plane/.

FROM rust:slim-bookworm AS builder

# aws-lc-rs (pulled in transitively via rustls) compiles a vendored C library and
# needs a C/C++ toolchain, cmake, and perl (for assembly codegen) in addition to cargo.
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential clang cmake perl pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY control-plane/ ./control-plane/
WORKDIR /workspace/control-plane
RUN cargo build --release -p shiftlite-bin

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 shiftlite

COPY --from=builder /workspace/control-plane/target/release/shiftlite-bin /usr/local/bin/shiftlite-bin

USER shiftlite
WORKDIR /home/shiftlite
ENV SHIFTLITE_CONFIG=/etc/shiftlite/config.yaml
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=5 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/shiftlite-bin"]
