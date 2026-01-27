FROM rust:1.83 as build

# create a new empty shell project
RUN USER=root cargo new --bin ruck
WORKDIR /ruck

# copy over manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# cache dependencies
RUN cargo build --release

# copy source tree
RUN rm src/*.rs
COPY ./src ./src

# build for release
RUN rm ./target/release/deps/ruck*
RUN cargo build --release

# minimal runtime image
FROM debian:bookworm-slim

# create non-root user
RUN useradd -r -u 1000 ruck

COPY --from=build /ruck/target/release/ruck /usr/local/bin/ruck

USER ruck

ENV RUST_LOG=info

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD nc -z localhost 8080 || exit 1

ENTRYPOINT ["/usr/local/bin/ruck"]
CMD ["relay", "--bind", "0.0.0.0:8080"]

