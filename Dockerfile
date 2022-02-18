FROM rust:1.58 as build

# create a new empty shell project
RUN USER=root cargo new --bin ruck
WORKDIR /ruck

# copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache your dependencies
RUN cargo build --release

# copy your source tree
RUN rm src/*.rs
COPY ./src ./src

# build for release
RUN rm ./target/release/deps/ruck*
RUN cargo build --release

# Copy the binary into a new container for a smaller docker image
FROM debian:buster-slim

COPY --from=build /ruck/target/release/ruck /
USER root

ENV RUST_LOG=info
ENV RUST_BACKTRACE=full

CMD ["/ruck", "relay"]

