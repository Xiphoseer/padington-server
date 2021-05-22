FROM rust:1.52.1-alpine AS builder
WORKDIR /usr/src/padington-server
RUN apk add musl-dev
COPY . .
RUN cargo build --release --no-default-features

FROM alpine:3.13
COPY --from=builder /usr/src/padington-server/target/release/padington-server /usr/local/bin/padington-server
RUN mkdir /srv/padington
RUN echo "padington-server --base-folder /srv/padington --port 9002" | tee run.sh
ENTRYPOINT ["sh", "run.sh"]
