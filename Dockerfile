# syntax=docker/dockerfile:1

FROM oven/bun:1 AS web-builder
WORKDIR /app/web

COPY web/package.json web/bun.lock* ./
RUN if [ -f bun.lockb ] || [ -f bun.lock ]; then bun install --frozen-lockfile; else bun install; fi

COPY web/ ./
ARG APP_EFFECTIVE_VERSION=0.0.0
ENV VITE_APP_VERSION=${APP_EFFECTIVE_VERSION}
RUN bun run build

FROM rust:1 AS rust-builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src/ ./src/
COPY --from=web-builder /app/web/dist ./web/dist
RUN cargo build --release --locked

FROM debian:13-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=rust-builder /app/target/release/catnap /app/catnap

ARG APP_EFFECTIVE_VERSION=0.0.0
ENV APP_EFFECTIVE_VERSION=${APP_EFFECTIVE_VERSION}
ENV BIND_ADDR=0.0.0.0:18080

EXPOSE 18080
CMD ["/app/catnap"]
