FROM rust:1.82-slim AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web
# web/ 必须在 include_str! 之前就位
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/hypercast-relay-directory /usr/local/bin/
ENV HC_DIRECTORY_BIND=0.0.0.0:8080
EXPOSE 8080
VOLUME /data
ENTRYPOINT ["hypercast-relay-directory"]
