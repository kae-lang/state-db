FROM rust:1.89-bookworm AS builder

WORKDIR /build

# Install dependencies needed by RocksDB bundled build
RUN apt-get update && apt-get install -y clang libclang-dev && rm -rf /var/lib/apt/lists/*

# Copy workspace
COPY smql-engine/ smql-engine/

# Build release binary with RocksDB + auth
WORKDIR /build/smql-engine
RUN cargo build --release --bin smql --features "rocksdb,auth"

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/smql-engine/target/release/smql /usr/local/bin/smql

RUN mkdir -p /data

EXPOSE 4200

ENTRYPOINT ["smql"]
CMD ["serve", "--bind", "0.0.0.0:4200", "--storage", "/data/smql.db"]
