# Project Super OSS — containerpi/super
#
# Build (from repository root):
#   docker build -f dockerbuild/Dockerfile -t containerpi/super:latest .
# Multi-arch (CI / publish):
#   docker buildx build --platform linux/amd64,linux/arm64 -f dockerbuild/Dockerfile -t containerpi/super:latest --push .
# Config is COPY'd from dockerbuild/conf/ into the image — no -v at build time.
#
# Run (default config already inside the image):
#   docker run --rm -p 127.0.0.1:9002:9002 containerpi/super:latest
#
# Run with your own conf/data (optional override):
#   docker run --rm -p 127.0.0.1:9002:9002 \
#     -v ./my-conf:/app/super/conf -v ./my-data:/app/super/data \
#     containerpi/super:latest
#
# OSS image: no embedded dashboard. Install the UI plugin from a subscription package for the web control plane.

FROM rust:1-trixie AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY common/ common/
COPY core/ core/
COPY cli/ cli/
COPY superd/ superd/
RUN cargo build --release -p superd -p super-cli

# tini is not included in distroless; copy from a minimal Debian stage (build-only).
FROM debian:13-slim AS tini
RUN apt-get update \
    && apt-get install -y --no-install-recommends tini \
    && rm -rf /var/lib/apt/lists/*

# Pre-create writable dirs for the nonroot distroless user (uid 65532).
FROM debian:13-slim AS layout
COPY dockerbuild/conf/ /app/super/conf/
RUN mkdir -p /app/super/data /app/super/logs \
    && chown -R 65532:65532 /app/super

# Static busybox for docs/quick-start demos inside the distroless runtime (httpd, etc.).
FROM debian:13-slim AS busybox
RUN apt-get update \
    && apt-get install -y --no-install-recommends busybox-static \
    && rm -rf /var/lib/apt/lists/*

# Runtime: distroless Debian 13 — minimal glibc/OpenSSL, no apt/Perl.
FROM gcr.io/distroless/cc-debian13:nonroot

LABEL org.opencontainers.image.title="Project Super"
LABEL org.opencontainers.image.description="API-first process orchestrator (OSS)"
LABEL org.opencontainers.image.source="https://github.com/schiplat/super"
LABEL org.opencontainers.image.vendor="623"

COPY --from=tini /usr/bin/tini /usr/bin/tini
COPY --from=backend /build/target/release/superd /usr/local/bin/superd
COPY --from=backend /build/target/release/super /usr/local/bin/super
COPY --from=busybox /bin/busybox /usr/local/bin/busybox
COPY --from=layout --chown=65532:65532 /app/super /app/super

ENV SUPER_ROOT=/app/super
WORKDIR /app/super

VOLUME ["/app/super/conf", "/app/super/data", "/app/super/logs"]
EXPOSE 9002

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/super", "list"]

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/superd"]
