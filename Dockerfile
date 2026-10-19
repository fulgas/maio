FROM rust:1-trixie AS build
RUN rustup target add wasm32-unknown-unknown && cargo install --locked cargo-leptos@0.3.5
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY style style
COPY public public
RUN cargo leptos build --release

FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/maio /usr/local/bin/maio
COPY --from=build /src/target/site /usr/local/share/maio/site
ENV LEPTOS_OUTPUT_NAME=maio LEPTOS_SITE_ROOT=/usr/local/share/maio/site LEPTOS_SITE_PKG_DIR=pkg LEPTOS_ENV=PROD
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["maio", "/etc/maio/maio.toml"]
