# syntax=docker/dockerfile:1.7

FROM rust:1.94-bookworm AS builder
WORKDIR /app

# Cache dependency compilation first.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY docs ./docs

RUN awk 'BEGIN{skip=0} /^\[workspace\]/{skip=1; next} /^\[.*\]/{if(skip){skip=0}} !skip{print}' Cargo.toml > Cargo.toml.docker \
    && mv Cargo.toml.docker Cargo.toml \
    && cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/osmium /usr/local/bin/osmium

ENV BIND_ADDR=0.0.0.0:3000
ENV RUN_MIGRATIONS_ON_STARTUP=true
EXPOSE 3000

CMD ["/usr/local/bin/osmium"]
