FROM rust:1.88-slim

WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src
COPY examples ./examples
RUN cargo build --release --locked

ENTRYPOINT ["/app/target/release/nemo"]
