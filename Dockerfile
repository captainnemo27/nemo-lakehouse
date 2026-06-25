FROM rust:1.88-slim

WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
COPY examples ./examples
RUN cargo build --release

ENTRYPOINT ["/app/target/release/nemo"]

