# rust-backend/Dockerfile
FROM rust:1.86.0

WORKDIR /app

# Copy source code (including .env)
COPY . .


RUN apt-get update && apt-get install -y pkg-config libssl-dev


COPY .env .env

# Build the release binary
RUN cargo build --release

# Run the app (adjust binary name if needed)
CMD ["/app/target/release/rustraptor-backend"]
