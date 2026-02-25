# ClawProof ZKML Docker build
#
# Build:
#   docker build -t clawproof .
#
# Run:
#   docker run -p 3000:3000 clawproof

# --- Builder stage ---
# Nightly required: arkworks-algebra dev/twist-shout branch uses const generics
# features that need Rust >= 1.95 nightly.
FROM debian:bookworm AS builder

# Install build dependencies and rustup
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install pinned nightly toolchain via rustup
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain nightly-2026-01-29 && \
    rustc --version

WORKDIR /build

# Limit parallel jobs to avoid OOM with ZKML deps
ENV CARGO_BUILD_JOBS=1

# Copy manifest + lockfile first for layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source so cargo can fetch & compile dependencies first (layer cache)
RUN mkdir -p src \
    && echo 'fn main(){}' > src/main.rs \
    && cargo build --release --bin clawproof || true \
    && rm -rf src

# Copy Rust source only (NOT models — those go in a later layer
# so model changes don't trigger a full Rust recompile)
COPY src/ src/

# Touch main.rs to force cargo to recompile the crate
RUN touch src/main.rs && cargo build --release --bin clawproof

# --- Python converter stage ---
FROM python:3.11-slim AS converter-builder
WORKDIR /converter
COPY converter/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY converter/main.py .

# --- Runtime stage ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    binutils \
    python3 \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /sbin/nologin clawproof

WORKDIR /app

# Copy binary
COPY --from=builder /build/target/release/clawproof /app/clawproof

# Strip release binary to reduce image size
RUN strip /app/clawproof

# Pre-generated Dory SRS files (avoids runtime generation which can OOM)
COPY dory_srs_22_variables.srs /app/dory_srs_22_variables.srs
COPY dory_srs_28_variables.srs /app/dory_srs_28_variables.srs

# Copy model files last — changes here only rebuild from this layer onward
COPY models/ /app/models/

# Copy Python converter
COPY --from=converter-builder /converter /app/converter
COPY --from=converter-builder /usr/local/lib/python3.11/site-packages /usr/local/lib/python3.11/dist-packages

# Create data directory for SQLite and uploaded models
RUN mkdir -p /app/data /app/data/models

# Create entrypoint script that starts both processes
RUN printf '#!/bin/sh\n\
# Start Python converter sidecar in background (if available)\n\
if [ -f /app/converter/main.py ]; then\n\
    python3 /app/converter/main.py &\n\
fi\n\
# Start Rust server\n\
exec /app/clawproof\n' > /app/entrypoint.sh && chmod +x /app/entrypoint.sh

RUN chown -R clawproof:clawproof /app

# Render sets PORT=10000 for web services
ENV PORT=10000
ENV MODELS_DIR=/app/models
ENV DATABASE_PATH=/app/data/clawproof.db
ENV UPLOADED_MODELS_DIR=/app/data/models
ENV CONVERTER_URL=http://127.0.0.1:8001
ENV RUST_LOG=info
EXPOSE 10000

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD curl -f http://localhost:${PORT}/health || exit 1

USER clawproof

CMD ["/app/entrypoint.sh"]
