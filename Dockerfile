FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo build --release --bin syncenvd --bin syncenv-cli

CMD ["./target/release/syncenvd"]
