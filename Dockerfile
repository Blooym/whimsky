###########
# Builder #
###########
FROM rust:alpine3.21 AS builder 
WORKDIR /build

# Install build dependencies
RUN apk add --update build-base cmake libressl-dev=4.0.0-r0

# Pre-cache dependencies
COPY ["Cargo.toml", "Cargo.lock", "./"]
RUN mkdir src \
    && echo "// Placeholder" > src/lib.rs \
    && cargo build --release \
    && rm src/lib.rs

# Build
ARG SQLX_OFFLINE true
COPY ./migrations ./migrations
COPY ./.sqlx ./.sqlx
COPY ["./src", "./src"]
RUN cargo build --release

###########
# Runtime #
###########
FROM alpine
RUN adduser -S -s /bin/false -D whimsky
USER whimsky
WORKDIR /opt/whimsky
RUN mkdir /opt/whimsky/data

ENV RUST_BACKTRACE=1
ENV DATABASE_URL=sqlite:///opt/whimsky/data/db.sqlite3?mode=rwc
ENV DATA_PATH=/opt/whimsky/data
COPY --from=builder /build/target/release/whimsky /usr/local/bin/whimsky
ENTRYPOINT ["/usr/local/bin/whimsky", "start"]