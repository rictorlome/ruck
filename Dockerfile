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
RUN rm -f ./target/release/deps/ruck_relay* ./target/release/ruck-relay
RUN cargo build --release

# minimal runtime image
FROM debian:bookworm-slim

# install runtime deps for healthcheck
RUN apt-get update \
    && apt-get install -y --no-install-recommends netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# create non-root user
RUN useradd -r -u 1000 ruck

COPY --from=build /ruck/target/release/ruck-relay /usr/local/bin/ruck-relay

USER ruck

ENV RUST_LOG=info

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD nc -z localhost 8080 || exit 1

ENTRYPOINT ["/usr/local/bin/ruck-relay"]
CMD ["relay", "--bind", "0.0.0.0:8080"]
