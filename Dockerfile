FROM rust:1.52.1-alpine AS builder
WORKDIR /usr/src/padington-server

# Needed for the statically linked libc in musl
RUN apk add musl-dev

# Copy a minimal cargo project to fetch dependencies into a layer
COPY Cargo.toml Cargo.lock ./
RUN mkdir src
RUN touch src/main.rs
RUN cargo fetch

# Build the app
COPY . .
RUN cargo build --release --no-default-features

# Use an alpine distribution
FROM alpine:3.13

# Set the labels
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
LABEL org.opencontainers.image.source="https://github.com/Xiphoseer/padington-server"
LABEL org.opencontainers.image.title="Padington (Server)"
LABEL org.opencontainers.image.description="Collaborative Text-Editor Backend"

# Set the environment
ENV PORT 9002
ENV BASE_FOLDER "/srv/padington"

# Prepare the output directory
RUN mkdir $BASE_FOLDER
# Copy the binary from the builder
COPY --from=builder /usr/src/padington-server/target/release/padington-server /usr/local/bin/padington-server

# Set the entry point
ENTRYPOINT padington-server --base-folder $BASE_FOLDER --port $PORT
