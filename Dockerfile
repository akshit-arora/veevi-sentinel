# Multi-stage Dockerfile for Veevi Sentinel Demo Sandbox
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /usr/src/veevi-sentinel
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Runtime image with pre-installed DB clients and demo sandbox
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    sqlite3 \
    postgresql-client \
    default-mysql-client \
    ca-certificates \
    curl \
    bash \
    && rm -rf /var/lib/apt/lists/*

# Install veevi-sentinel binary
COPY --from=builder /usr/src/veevi-sentinel/target/release/veevi-sentinel /usr/local/bin/veevi-sentinel

# Set up demo sandbox environment
WORKDIR /demo
COPY demo/config.yaml /demo/config.yaml
COPY demo/init-db.sql /demo/init-db.sql
COPY demo/welcome.sh /demo/welcome.sh

RUN chmod +x /demo/welcome.sh

ENTRYPOINT ["/demo/welcome.sh"]
CMD ["/bin/bash"]
