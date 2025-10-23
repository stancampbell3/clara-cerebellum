FROM rust:1.82-slim as builder
WORKDIR /build
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/clara-api /usr/local/bin/
COPY ./clips/binaries/clips /usr/local/bin/
COPY ./config /etc/clara-cerebrum/
EXPOSE 8080 9090
CMD ["clara-api"]
