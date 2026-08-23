FROM rust:1.88-alpine3.22 AS builder

RUN apk add --no-cache musl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM scratch
ARG BUILD_VERSION
ARG BUILD_ARCH
LABEL io.hass.version="${BUILD_VERSION}" \
      io.hass.type="app" \
      io.hass.arch="${BUILD_ARCH}"
COPY --from=builder /build/target/release/vistoda-blink-engine /vistoda-blink-engine
ENTRYPOINT ["/vistoda-blink-engine"]
