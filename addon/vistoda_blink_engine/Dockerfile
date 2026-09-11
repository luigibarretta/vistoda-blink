FROM rust:1.88-alpine3.22@sha256:9dfaae478ecd298b6b5a039e1f2cc4fc040fc818a2de9aa78fa714dea036574d AS builder

RUN apk add --no-cache musl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

COPY collect-licenses.sh /usr/local/bin/collect-licenses
RUN sh /usr/local/bin/collect-licenses /licenses

FROM alpine:3.24@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b
ARG BUILD_VERSION
ARG BUILD_ARCH
LABEL io.hass.version="${BUILD_VERSION}" \
      io.hass.type="app" \
      io.hass.arch="${BUILD_ARCH}" \
      org.opencontainers.image.source="https://github.com/luigibarretta/vistoda-blink" \
      org.opencontainers.image.licenses="MIT AND Apache-2.0"
RUN apk add --no-cache ca-certificates curl jq su-exec \
    && addgroup -g 10001 bridge \
    && adduser -D -H -u 10001 -G bridge bridge
COPY --from=builder /licenses /usr/share/doc/vistoda/dependencies
COPY licenses/ /usr/share/doc/vistoda/
COPY --from=builder /build/target/release/vistoda-blink-engine /usr/local/bin/
COPY rootfs/run.sh /usr/local/bin/vistoda-app-run
COPY rootfs/vistoda-app-bootstrap.sh /usr/local/lib/vistoda-app-bootstrap
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:8099/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/vistoda-app-run"]
