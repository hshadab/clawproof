# ClawProof

zkML proof-as-a-service. Run ML inference on ONNX models and get cryptographic proof receipts that anyone can verify — no API keys, no auth, just call the endpoint. Working prototype.

**Proof system:** [JOLT-Atlas](https://github.com/ICME-Lab/jolt-atlas) SNARK with Dory commitment scheme on BN254

**Live:** https://clawproof.onrender.com

**Skill:** [`SKILL.md`](https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md) — install in any OpenClaw agent

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

## Why agents need this

If your agent runs ML inference to make decisions — classification, authorization, risk scoring — you have a trust problem. Other agents and humans can't verify your output was legitimate without re-running the model themselves.

ClawProof solves this:

- **Verifiable decisions** — Prove your ML output is correct with a cryptographic SNARK. No one needs to trust you; they can verify mathematically.
- **Privacy-preserving** — Prove correctness without revealing model weights or private inputs.
- **Accountability receipts** — Every proof generates a receipt with Keccak256 hashes of model, input, and output. Non-repudiable evidence of what your agent decided and why.
- **Composable trust** — Other agents verify your proof in ~80ms without running inference. Chain verified decisions across multi-agent workflows.
- **No auth** — No API keys, no signup, no cost. Agents can self-serve autonomously.
- **Bring Your Own Model** — Upload any ONNX model (up to 5MB) and get SNARK proofs for your own architecture.

Use case examples: an authorization agent proves it ran the model correctly before approving a transaction. A content moderation agent proves its classification. A trading agent proves its risk score. Any downstream agent or auditor can verify in milliseconds.

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

Convert PyTorch (.pt), TensorFlow (.pb), or sklearn (.pkl) models to ONNX. Requires the converter sidecar (`CONVERTER_URL`). Conversion produces ONNX but does not guarantee the model fits within the 5MB file size limit or trace length budget.

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

| Model | Type | Input | Output | Trace |
|-------|------|-------|--------|-------|
| `authorization` | Structured fields | 8 fields (budget, trust, amount, category, velocity, day, time, risk) | AUTHORIZED / DENIED | 2^14 |
| `sentiment` | Text (TF-IDF) | News article text (512-dim TF-IDF vector) | BUSINESS / ENTERTAINMENT / POLITICS / SPORT / TECH | 2^14 |
| `spam_detector` | Raw vector | 8-dimensional integer vector | CLASS_0 / CLASS_1 / CLASS_2 / CLASS_3 | 2^12 |

## Supported ONNX operations

ClawProof uses the [JOLT-Atlas](https://github.com/ICME-Lab/jolt-atlas) proving system. The atlas-onnx-tracer compiles ONNX graphs into provable circuits. The operators listed below are from the actual source code.

**Atlas-onnx-tracer `Operator` enum (26 ops):**

`Add` · `Sub` · `Mul` · `Div` · `Neg` · `ReLU` · `Tanh` · `Erf` · `SoftmaxAxes` · `Einsum` (covers MatMul, Gemm, batched attention) · `Sum` · `Reshape` · `Broadcast` · `MoveAxis` · `Gather` · `Identity` · `Constant` · `Input` · `Square` · `Cube` · `Rsqrt` · `ScalarConstDiv` · `Clamp` · `And` · `Iff` · `IsNan`

**Extended ops (onnx-tracer pipeline):**

The full onnx-tracer supports a wider set via polynomial, lookup-table, and hybrid op categories:

- **Arithmetic:** `Add`, `Sub`, `Mul`, `Div`, `Neg`, `Pow`, `Prod`
- **Activations:** `ReLU`, `LeakyReLU`, `Sigmoid`, `Tanh`, `Erf`
- **Trig:** `Sin`, `Cos`, `Tan`, `ASin`, `ACos`, `ATan`, `Sinh`, `Cosh`
- **Math:** `Exp`, `Ln`, `Sqrt`, `Rsqrt`, `Abs`, `Sign`, `Ceil`, `Floor`, `Round`
- **Comparison:** `Greater`, `GreaterEqual`, `Less`, `LessEqual`, `Equals`
- **Reduction:** `Sum`, `ReduceMax`, `ReduceMin`, `ReduceArgMax`, `ReduceArgMin`, `TopK`
- **Pooling:** `MaxPool2d`, `SumPool`, `GlobalSumPool`
- **Conv:** `Conv`, `DeConv`, `Downsample`
- **Shape:** `Reshape`, `Flatten`, `MoveAxis`, `Pad`, `Concat`, `Slice`, `Resize`
- **Indexing:** `Gather`, `GatherElements`, `ScatterElements`, `OneHot`
- **Logic:** `And`, `Or`, `Xor`, `Not`, `Iff`
- **Attention:** `Einsum`, `Softmax`

**Model constraints:**
- ONNX format only (convert from PyTorch/TensorFlow/sklearn via `/convert`)
- Max ONNX file size: 5MB
- All inputs are cast to integer (i32) tensors
- Trace length must accommodate model complexity (default 2^14)

**Conversion notes (`/convert`):**
- Conversion produces ONNX but does not guarantee the model fits in the trace length budget
- PyTorch models should be traced (`torch.jit.trace`) before upload
- sklearn models: LogisticRegression, MLPClassifier, DecisionTree, etc.
- TensorFlow/Keras models with custom layers may fail conversion

For the full operator source, see [atlas-onnx-tracer/src/ops](https://github.com/ICME-Lab/jolt-atlas/tree/main/atlas-onnx-tracer/src/ops) and [onnx-tracer/src/ops](https://github.com/ICME-Lab/jolt-atlas/tree/main/onnx-tracer/src/ops).

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

## For Moltbook agents

ClawProof is built for autonomous agent adoption. Any agent that makes ML-based decisions can use ClawProof to produce verifiable proof of correct inference — no API keys, no cost, no human in the loop.

Install the skill and start proving inference:

```
https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md
```

Your agent gets:
- **One-call proof generation** — `POST /prove` with model ID and input, get a receipt back
- **Verifiable receipts** — Keccak256 hashes of model, input, output, and SNARK proof
- **Proof badges** — Embeddable SVG badges for posts and dashboards
- **BYOM** — Upload your own ONNX model and prove inference on it
- **Composable verification** — Any other agent can verify your proof in ~80ms without re-running the model

Agents that prove their decisions build trust. Agents that don't are asking others to take their word for it.

See [`moltbook/`](moltbook/) for posting strategy and ready-to-use Skill Drop posts.

Operated by [@hshadab](https://www.moltbook.com/u/skillguard-agent).

## License

MIT
