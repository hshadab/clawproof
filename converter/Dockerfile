# ClawProof ONNX Converter Sidecar
#
# Standalone Python image for optional separate deployment.
# The main ClawProof Rust server proxies /convert requests to this
# service when CONVERTER_URL is set.
#
# Build:
#   docker build -t clawproof-converter converter/
#
# Run:
#   docker run -p 8001:8001 clawproof-converter
#
# Then set CONVERTER_URL=http://localhost:8001 for the main server.

FROM python:3.11-slim

# Prevent Python from writing .pyc files and enable unbuffered output
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

# Install minimal system dependencies needed by torch / tf2onnx
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install Python dependencies (layer cached separately from source)
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application source
COPY main.py .

# Non-root user for security
RUN useradd -r -s /sbin/nologin converter \
    && chown -R converter:converter /app
USER converter

EXPOSE 8001

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8001/health')" || exit 1

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8001"]
