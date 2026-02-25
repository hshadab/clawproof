# clawproof (ClawProof)

zkML proof-as-a-service. Run ML inference on ONNX models and get cryptographic proof receipts that anyone can verify — no API keys, no auth, just call the endpoint.

**Proof system:** JOLT-Atlas SNARK with Dory commitment scheme on BN254

**Live:** https://clawproof.onrender.com

## Quick start

```bash
git clone https://github.com/hshadab/clawproof.git
cd clawproof
cp .env.example .env
cargo run --release
# Server starts at http://localhost:3000
```

Or with Docker:

```bash
docker build -t clawproof .
docker run -p 3000:3000 clawproof
```

## How it works

1. Submit input to a model (text, structured fields, or raw vector)
2. ClawProof runs inference and generates a JOLT-Atlas SNARK proof
3. You get a receipt with cryptographic hashes (Keccak256) of model, input, and output
4. Anyone can verify the proof without seeing private data

## SDKs

### Python

```bash
pip install clawproof
```

```python
from clawproof import ClawProof

pg = ClawProof()  # defaults to https://clawproof.onrender.com
receipt = pg.prove_and_wait("authorization",
    fields={"budget": 5, "trust": 3, "amount": 8,
            "category": 1, "velocity": 2, "day": 3,
            "time": 1, "risk": 0})
print(receipt.output.label)
```

### JavaScript / TypeScript

```bash
npm install clawproof
```

```typescript
import { ClawProof } from "clawproof";

const pg = new ClawProof();
const receipt = await pg.proveAndWait("authorization", {
  fields: { budget: 5, trust: 3, amount: 8,
            category: 1, velocity: 2, day: 3,
            time: 1, risk: 0 }
});
console.log(receipt.output.label);
```

## MCP (Claude Desktop)

ClawProof ships an MCP server for agent integration. Add to your Claude Desktop config:

```json
{
  "mcpServers": {
    "clawproof": {
      "command": "clawproof-mcp",
      "env": {
        "CLAWPROOF_URL": "https://clawproof.onrender.com"
      }
    }
  }
}
```

Build the MCP server:

```bash
cd mcp-server
cargo build --release
# Binary at mcp-server/target/release/clawproof-mcp
```

Tools exposed: `list_models`, `prove`, `verify`, `get_receipt`, `upload_model`.

## API

Full OpenAPI 3.1 spec at [`/openapi.json`](https://clawproof.onrender.com/openapi.json).

### `POST /prove`

Generate a proof. Returns immediately; proof generation continues in background.

```bash
# Structured input
curl -X POST https://clawproof.onrender.com/prove \
  -H "Content-Type: application/json" \
  -d '{
    "model_id": "authorization",
    "input": {
      "fields": {
        "budget": 10, "trust": 5, "amount": 8,
        "category": 2, "velocity": 3, "day": 1,
        "time": 2, "risk": 0
      }
    }
  }'

# With webhook callback
curl -X POST https://clawproof.onrender.com/prove \
  -H "Content-Type: application/json" \
  -d '{
    "model_id": "authorization",
    "input": { "fields": { "budget": 10, "trust": 5, "amount": 8, "category": 2, "velocity": 3, "day": 1, "time": 2, "risk": 0 } },
    "webhook_url": "https://your-server.com/callback"
  }'
```

Response:
```json
{
  "receipt_id": "abc-123",
  "receipt_url": "https://clawproof.onrender.com/receipt/abc-123",
  "model_id": "authorization",
  "output": {
    "raw_output": [142, -89],
    "predicted_class": 0,
    "label": "AUTHORIZED",
    "confidence": 0.614
  },
  "status": "proving"
}
```

### `POST /prove/batch`

Prove up to 5 inputs at once.

```bash
curl -X POST https://clawproof.onrender.com/prove/batch \
  -H "Content-Type: application/json" \
  -d '{"requests": [
    {"model_id": "authorization", "input": {"fields": {"budget": 5, "trust": 3, "amount": 8, "category": 1, "velocity": 2, "day": 3, "time": 1, "risk": 0}}},
    {"model_id": "authorization", "input": {"fields": {"budget": 15, "trust": 1, "amount": 12, "category": 3, "velocity": 5, "day": 0, "time": 3, "risk": 1}}}
  ]}'
```

### `GET /receipt/{id}`

Retrieve a proof receipt. Returns HTML by default, JSON with `Accept: application/json`, or JSON-LD with `?format=jsonld`.

```bash
curl -H "Accept: application/json" https://clawproof.onrender.com/receipt/abc-123
curl https://clawproof.onrender.com/receipt/abc-123?format=jsonld
```

### `POST /verify`

```bash
curl -X POST https://clawproof.onrender.com/verify \
  -H "Content-Type: application/json" \
  -d '{"receipt_id": "abc-123"}'
```

### `GET /badge/{receipt_id}`

SVG badge for embedding in docs or dashboards:

```markdown
![proof](https://clawproof.onrender.com/badge/{receipt_id})
```

### `GET /metrics`

Aggregate platform stats (total proofs, verified count, avg timing, per-model breakdown).

### `POST /models/upload`

Upload a custom ONNX model (max 5MB). The model will be registered and preprocessed for proof generation.

```bash
curl -X POST https://clawproof.onrender.com/models/upload \
  -F "onnx_file=@model.onnx" \
  -F "name=My Model" \
  -F "input_dim=64" \
  -F 'labels=["class_a","class_b"]' \
  -F "trace_length=16384"
```

### `POST /convert`

Convert PyTorch (.pt), TensorFlow (.pb), or sklearn (.pkl) models to ONNX. Requires the converter sidecar (`CONVERTER_URL`).

### All endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/models` | List available models |
| `POST` | `/prove` | Submit proof request |
| `POST` | `/prove/batch` | Batch prove (max 5) |
| `GET` | `/receipt/{id}` | Get proof receipt |
| `POST` | `/verify` | Verify a proof |
| `GET` | `/metrics` | Platform metrics |
| `GET` | `/badge/{id}` | SVG proof badge |
| `POST` | `/models/upload` | Upload ONNX model |
| `POST` | `/convert` | Convert model to ONNX |
| `GET` | `/openapi.json` | OpenAPI 3.1 spec |

## Built-in models

| Model | Input | Output | Trace |
|-------|-------|--------|-------|
| `authorization` | 8 structured fields (budget, trust, amount, category, velocity, day, time, risk) | AUTHORIZED / DENIED | 2^14 |

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3000` | HTTP server port |
| `MODELS_DIR` | `./models` | Path to built-in ONNX models |
| `BASE_URL` | `http://localhost:{PORT}` | Public URL for receipt links |
| `DATABASE_PATH` | `./data/clawproof.db` | SQLite database for persistent receipts |
| `UPLOADED_MODELS_DIR` | `./data/models` | Storage for user-uploaded models |
| `CONVERTER_URL` | — | Python converter sidecar URL |
| `CORS_ORIGINS` | `*` (any) | Comma-separated allowed origins |
| `RUST_LOG` | `info` | Log level |

## Architecture

```
POST /prove → Inference → Receipt (status: "proving") → [webhook callback]
                               ↓
                         Background task:
                         SNARK proof generation → Verification
                               ↓
                         Receipt updated (status: "verified")
                         Persisted to SQLite
```

- **Proof system:** JOLT-Atlas (lookup-based SNARK)
- **Commitment:** Dory vector commitment on BN254
- **Hashing:** Keccak256 for model/input/output/proof commitments
- **Storage:** SQLite (persistent) + DashMap (in-memory hot cache)
- **Model registry:** TOML-based, supports runtime additions via upload

## Rate limits

| Endpoint | Limit |
|----------|-------|
| `POST /prove` | 10 / 60s |
| `POST /prove/batch` | 2 / 60s |
| `POST /models/upload` | 1 / 300s |

## License

MIT
