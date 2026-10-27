FROM rust:1-trixie@sha256:931c2c0531a8b143e044f0f3914dfe802f15bb3d1955f0b657e9359327ac2ffc AS build
RUN rustup target add wasm32-unknown-unknown && cargo install --locked cargo-leptos@0.3.11
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY style style
COPY public public
RUN cargo leptos build --release

FROM debian:trixie-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad
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
