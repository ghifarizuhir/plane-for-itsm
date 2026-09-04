FROM rust:1.78-alpine AS builder
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
WORKDIR /build
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --release --bin api --bin worker --bin beat

FROM alpine:3.19
RUN apk add --no-cache ca-certificates libgcc
COPY --from=builder /build/target/release/api /usr/local/bin/api
COPY --from=builder /build/target/release/worker /usr/local/bin/worker
COPY --from=builder /build/target/release/beat /usr/local/bin/beat
EXPOSE 8000
ENV MALLOC_CONF="dirty_decay_ms:1000"
