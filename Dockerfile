FROM rust:1-trixie@sha256:223cc32ddca8f3ee90c436dbc94b6b5f052ea7f9653c5201e9612f08ad7d21b8 AS build
RUN rustup target add wasm32-unknown-unknown && cargo install --locked cargo-leptos@0.3.11
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY style style
COPY public public
RUN cargo leptos build --release

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/maio /usr/local/bin/maio
# The asset hashes sit next to the binary, where Leptos looks for them.
COPY --from=build /src/target/release/hash.txt /usr/local/bin/hash.txt
COPY --from=build /src/target/site /usr/local/share/maio/site
ENV LEPTOS_OUTPUT_NAME=maio LEPTOS_SITE_ROOT=/usr/local/share/maio/site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD LEPTOS_HASH_FILES=true
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["maio", "/etc/maio/maio.toml"]
