FROM alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce AS audio-codec
RUN apk add --no-cache build-base curl perl xz
COPY build-audio-codec.sh /usr/local/bin/build-audio-codec
RUN sh /usr/local/bin/build-audio-codec

FROM rust:1.88-alpine3.22@sha256:9dfaae478ecd298b6b5a039e1f2cc4fc040fc818a2de9aa78fa714dea036574d AS builder

RUN apk add --no-cache musl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

COPY collect-licenses.sh /usr/local/bin/collect-licenses
RUN sh /usr/local/bin/collect-licenses /licenses

FROM alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce
ARG BUILD_VERSION
ARG BUILD_ARCH
LABEL io.hass.version="${BUILD_VERSION}" \
      io.hass.type="app" \
      io.hass.arch="${BUILD_ARCH}" \
      org.opencontainers.image.source="https://github.com/luigibarretta/vistoda-blink" \
      org.opencontainers.image.licenses="MIT AND Apache-2.0 AND LGPL-2.1-or-later"
RUN apk add --no-cache ca-certificates curl jq su-exec \
    && addgroup -g 10001 bridge \
    && adduser -D -H -u 10001 -G bridge bridge
COPY --from=builder /licenses /usr/share/doc/vistoda/dependencies
COPY licenses/ /usr/share/doc/vistoda/
COPY --from=builder /build/target/release/vistoda-blink-engine /usr/local/bin/
COPY --from=audio-codec /audio-output/bin/ffmpeg /usr/local/bin/ffmpeg
COPY --from=audio-codec /audio-output/sources /usr/share/doc/vistoda/ffmpeg
COPY rootfs/run.sh /usr/local/bin/vistoda-app-run
COPY rootfs/vistoda-app-bootstrap.sh /usr/local/lib/vistoda-app-bootstrap
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:8099/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/vistoda-app-run"]
