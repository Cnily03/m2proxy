FROM rust:1.89-alpine AS builder

WORKDIR /app

RUN update-ca-certificates
RUN apk add --no-cache openssl-dev openssl-libs-static musl-dev pkgconfig clang lld

COPY .cargo ./.cargo
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --bin m2proxy --release --target x86_64-unknown-linux-musl && \
    mkdir -p /usr/local/bin && \
    cp target/x86_64-unknown-linux-musl/release/m2proxy /usr/local/bin/m2proxy

FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /usr/local/bin/m2proxy /m2proxy

EXPOSE 1234

ENTRYPOINT ["/m2proxy"]
