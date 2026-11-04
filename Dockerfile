# syntax=docker/dockerfile:1
# Everything here works with Docker alone, no Rust toolchain needed:
#   docker build --target test .                       fmt, clippy and tests
#   docker build -t attempt-planner . \
#     && docker run --rm -p 8000:80 attempt-planner    site on http://localhost:8000
#   docker build --target pkg --output web/pkg .       WebAssembly package only

FROM rust:1 AS toolchain
RUN rustup component add clippy rustfmt \
    && rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --locked
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates

FROM toolchain AS test
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo fmt --all --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace

FROM toolchain AS build
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/root/.cache/.wasm-pack \
    --mount=type=cache,target=/src/target \
    wasm-pack build crates/sim-wasm --release --target web --out-dir /pkg

FROM scratch AS pkg
COPY --from=build /pkg /

FROM nginx:alpine AS site
COPY web /usr/share/nginx/html
COPY --from=build /pkg /usr/share/nginx/html/pkg
