# cargo-chef: dependency layer di-cache terpisah dari source.
# - Ubah .rs saja -> cuma stage `builder` akhir yang jalan.
# - Ubah Cargo.toml/lock -> `planner`+`chef-cook` ikut rebuild (full).
# Dev iterasi API-only (jauh lebih cepat, skip link worker/beat):
#   docker compose build --build-arg BINS=api api && docker compose up -d api
# Dev tercepat (binary lebih besar, jangan prod): tambah
#   --build-arg RELEASE_LTO=false --build-arg RELEASE_CGU=16
# Default (prod/compose biasa) tetap build semua binary dengan LTO penuh.
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
# Knob LTO/codegen (lihat builder di bawah): HARUS identik di cook & build
# agar fingerprint deps sama. Default = prod, identik dengan
# [profile.release] di Cargo.toml.
ARG RELEASE_LTO=true
ARG RELEASE_CGU=1
ENV CARGO_PROFILE_RELEASE_LTO=$RELEASE_LTO \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=$RELEASE_CGU
RUN cargo chef cook --release --recipe-path recipe.json --bin api --bin worker --bin beat

FROM chef AS builder
# Subset binary untuk iterasi cepat (default: semua, sama seperti sebelumnya).
# chef-cook di atas tetap cook superset deps sehingga cache tetap kepakai
# baik untuk build full maupun subset.
ARG BINS="api worker beat"
# Longgarkan LTO/codegen untuk iterasi dev (link jauh lebih cepat, binary
# lebih besar & sedikit lebih lambat — JANGAN untuk prod). WAJIB sama dengan
# nilai di stage chef-cook di atas (sudah begitu: ARG yang sama diulang di
# tiap stage). Contoh dev:
#   docker compose build --build-arg BINS=api \
#     --build-arg RELEASE_LTO=false --build-arg RELEASE_CGU=16 api \
#     && docker compose up -d api
ARG RELEASE_LTO=true
ARG RELEASE_CGU=1
ENV CARGO_PROFILE_RELEASE_LTO=$RELEASE_LTO \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=$RELEASE_CGU
COPY --from=chef-cook /build/target /build/target
COPY --from=chef-cook /usr/local/cargo /usr/local/cargo
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release $(for b in $BINS; do printf -- "--bin %s " "$b"; done) \
 && mkdir -p /out \
 && for b in $BINS; do cp "/build/target/release/$b" /out/; done

FROM alpine:3.19
RUN apk add --no-cache ca-certificates libgcc
COPY --from=builder /out/ /usr/local/bin/
EXPOSE 8000
ENV MALLOC_CONF="dirty_decay_ms:1000"
