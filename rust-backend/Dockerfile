# rust-backend/Dockerfile
FROM rust:1.86.0

WORKDIR /app
COPY . .

RUN apt-get update && apt-get install -y pkg-config libssl-dev
RUN cargo build --release

CMD ["./target/release/RustRaptorBackend"]  # will change once you create the binary
