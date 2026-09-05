# cargo-chef: dependency layer di-cache terpisah dari source.
# - Ubah .rs saja -> cuma stage `builder` akhir yang jalan (~1-3 mnt).
# - Ubah Cargo.toml/lock -> `planner`+`chef-cook` ikut rebuild (full ~16 mnt).
FROM rust:1.96-alpine AS chef
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static build-base make perl \
 && cargo install cargo-chef --locked
WORKDIR /build
# Sinkronkan toolchain+components SEKALI di sini agar stage turunan tak re-download.
COPY rust-toolchain.toml ./
RUN rustup show

FROM chef AS planner
# HANYA manifest: recipe.json hanya berubah bila dependensi berubah.
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY crates/api/Cargo.toml crates/api/Cargo.toml
COPY crates/beat/Cargo.toml crates/beat/Cargo.toml
COPY crates/common/Cargo.toml crates/common/Cargo.toml
COPY crates/worker/Cargo.toml crates/worker/Cargo.toml
# cargo metadata butuh minimal satu file target per member (isi diabaikan).
RUN mkdir -p crates/api/src crates/beat/src crates/common/src crates/worker/src \
 && touch crates/api/src/lib.rs crates/api/src/main.rs crates/beat/src/main.rs crates/common/src/lib.rs crates/worker/src/main.rs
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS chef-cook
COPY --from=planner /build/recipe.json recipe.json
# HANYA manifest (bukan source): perubahan .rs tidak boleh invalidate layer ini.
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY crates/api/Cargo.toml crates/api/Cargo.toml
COPY crates/beat/Cargo.toml crates/beat/Cargo.toml
COPY crates/common/Cargo.toml crates/common/Cargo.toml
COPY crates/worker/Cargo.toml crates/worker/Cargo.toml
RUN mkdir -p crates/api/src crates/beat/src crates/common/src crates/worker/src \
 && touch crates/api/src/lib.rs crates/api/src/main.rs crates/beat/src/main.rs crates/common/src/lib.rs crates/worker/src/main.rs
RUN cargo chef cook --release --recipe-path recipe.json --bin api --bin worker --bin beat

FROM chef AS builder
COPY --from=chef-cook /build/target /build/target
COPY --from=chef-cook /usr/local/cargo /usr/local/cargo
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release --bin api --bin worker --bin beat

FROM alpine:3.19
RUN apk add --no-cache ca-certificates libgcc
COPY --from=builder /build/target/release/api /usr/local/bin/api
COPY --from=builder /build/target/release/worker /usr/local/bin/worker
COPY --from=builder /build/target/release/beat /usr/local/bin/beat
EXPOSE 8000
ENV MALLOC_CONF="dirty_decay_ms:1000"
