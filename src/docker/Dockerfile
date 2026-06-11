# ── Stage 1: Toolchain + dependency planner ────────────────────
# cargo-chef keeps dependency compilation in a stable layer so CI,
# release builds, and local rebuilds don't recompile the full Rust
# graph whenever only application sources change. Keep the builder on
# bookworm as well so the runtime image gets a compatible glibc.
FROM rust:slim-bookworm AS chef

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    git \
    libasound2-dev \
    libclang-dev \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install stable --profile minimal --component rustfmt --component clippy \
    && cargo install cargo-chef --locked --version 0.1.77

WORKDIR /build

FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

ARG CARGO_PROFILE_RELEASE_LTO=fat
ENV CARGO_PROFILE_RELEASE_LTO=${CARGO_PROFILE_RELEASE_LTO}

COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --locked --release --all-features --recipe-path recipe.json

COPY . .
RUN cargo build --locked --release --all-features \
    && strip target/release/stemcell

# ── Stage 2: Runtime ───────────────────────────────────────────
FROM debian:bookworm-slim

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    libasound2 \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash stemcell

# Copy binary from builder
COPY --from=builder /build/target/release/stemcell /usr/local/bin/stemcell

# Workspace and config persist via volume
VOLUME /home/stemcell/.stemcell

USER stemcell
WORKDIR /home/stemcell

ENTRYPOINT ["stemcell"]
