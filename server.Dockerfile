# HTTP API image: liteparse-server (Rust, axum) with Tesseract OCR.
# Build: docker build -f server.Dockerfile -t liteparse-server .
FROM rust:1-bookworm AS builder

RUN apt-get update && apt-get install -y \
    libclang-dev \
    libtesseract-dev \
    libleptonica-dev \
    cmake \
    g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

RUN cargo build --release -p liteparse-server \
    && mkdir /out \
    && cp "$(find /root/.cache/pdfium-rs -name libpdfium.so | head -1)" /out/

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libtesseract5 \
    liblept5 \
    tesseract-ocr-eng \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home liteparse

COPY --from=builder /build/target/release/liteparse-server /usr/local/bin/liteparse-server
# pdfium shared library
COPY --from=builder /out/libpdfium.so /usr/local/lib/pdfium/libpdfium.so
ENV PDFIUM_LIB_PATH="/usr/local/lib/pdfium"

USER liteparse
ENV PORT=8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD curl -fs http://localhost:${PORT}/health || exit 1
CMD ["liteparse-server"]
