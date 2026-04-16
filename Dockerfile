# ---- Build stage ----
FROM rust:1.87-slim AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release --bin hjarne-api

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*



WORKDIR /app
COPY --from=builder /app/target/release/hjarne-api .

EXPOSE 3000
CMD ["./hjarne-api"]