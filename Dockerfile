# ClawProof ZKML Docker build
#
# Uses cargo-chef for dependency caching: if only src/ files change,
# the expensive dependency compilation layer is reused from cache.
#
# Build:
#   docker build -t clawproof .
#
# Run:
#   docker run -p 3000:3000 clawproof

# --- Chef stage: install cargo-chef ---
FROM debian:bookworm AS chef

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Nightly required: arkworks-algebra dev/twist-shout branch uses const generics
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain nightly-2026-01-29 && \
    rustc --version

RUN cargo install cargo-chef --locked

WORKDIR /build

# --- Planner stage: compute the dependency recipe ---
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY static/ static/
RUN cargo chef prepare --recipe-path recipe.json

# --- Builder stage: compile deps (cached), then source ---
FROM chef AS builder

# Limit parallel jobs to avoid OOM with ZKML deps
ENV CARGO_BUILD_JOBS=1

# Cook dependencies (only re-runs when Cargo.toml/Cargo.lock change)
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Now copy source and build (only recompiles crate code)
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY static/ static/
RUN cargo build --release --bin clawproof

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

# Copy static assets (HTML/CSS/JS — editable without recompiling Rust)
COPY static/ /app/static/

# Copy model files — changes here only rebuild from this layer onward
COPY models/ /app/models/

# Copy Python converter
COPY --from=converter-builder /converter /app/converter
COPY --from=converter-builder /usr/local/lib/python3.11/site-packages /usr/local/lib/python3.11/dist-packages

# Create data directory for SQLite, uploaded models, and live static overrides
RUN mkdir -p /app/data /app/data/models /app/data/static

# Create entrypoint script
# - Seeds static files to persistent disk (only if not already present)
# - Starts the Rust server
RUN printf '#!/bin/sh\n\
# Seed static files to persistent disk on first boot.\n\
# To update the UI without redeploying, edit /app/data/static/playground.html\n\
# on the persistent disk directly.\n\
if [ ! -f /app/data/static/playground.html ]; then\n\
    cp /app/static/* /app/data/static/ 2>/dev/null || true\n\
fi\n\
# Start Rust server\n\
exec /app/clawproof\n' > /app/entrypoint.sh && chmod +x /app/entrypoint.sh

RUN chown -R clawproof:clawproof /app

# Render sets PORT=10000 for web services
ENV PORT=10000
ENV MODELS_DIR=/app/models
ENV STATIC_DIR=/app/data/static
ENV DATABASE_PATH=/app/data/clawproof.db
ENV UPLOADED_MODELS_DIR=/app/data/models
ENV CONVERTER_URL=http://127.0.0.1:8001
ENV RUST_LOG=info
EXPOSE 10000

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD curl -f http://localhost:${PORT}/health || exit 1

USER clawproof

CMD ["/app/entrypoint.sh"]
