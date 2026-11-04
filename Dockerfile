# syntax=docker/dockerfile:1
# Everything here works with Docker alone, no Rust or Node needed:
#   docker build --target test .                       fmt, clippy, tests, WebAssembly tests in Node
#   docker build --target web .                        page checks, tests and build
#   docker build -t attempt-planner . \
#     && docker run --rm -p 8000:80 attempt-planner    site on http://localhost:8000
#   docker build --target pkg --output web/pkg .       WebAssembly package only

FROM rust:1 AS toolchain
RUN apt-get update && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy rustfmt \
    && rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --locked
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates

FROM toolchain AS test
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/root/.cache/.wasm-pack \
    --mount=type=cache,target=/src/target \
    cargo fmt --all --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace \
    && wasm-pack test --node --release crates/sim-wasm

FROM toolchain AS build
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/root/.cache/.wasm-pack \
    --mount=type=cache,target=/src/target \
    wasm-pack build crates/sim-wasm --release --target web --out-dir /pkg

FROM scratch AS pkg
COPY --from=build /pkg /

FROM node:24-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web .
COPY --from=build /pkg pkg
RUN npm run check && npm test && npm run build

FROM nginx:alpine AS site
COPY --from=web /web/dist /usr/share/nginx/html
