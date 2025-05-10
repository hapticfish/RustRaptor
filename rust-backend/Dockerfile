

# === BUILD ===  #

FROM rust:1.86.0-bullseye AS builder

RUN apt-get update -qq && apt-get install -y -qq curl

WORKDIR /app

ENV SQLX_OFFLINE=true

COPY Cargo.toml Cargo.lock ./

COPY .sqlx ./sqlx

COPY src ./src
COPY tests ./tests
COPY .env .env

COPY . .

RUN cargo build --release


# === RUNTIME ===#

FROM debian:bullseye-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    apt-get clean

WORKDIR /app
RUN useradd -m appuser

COPY --from=builder /app/target/release/rustraptor-backend /usr/local/bin/rustraptor-backend
COPY .env .env

USER appuser
EXPOSE 8080
CMD ["rustraptor-backend"]
