---
name: clawproof
description: Generate and verify zero-knowledge proofs of ML inference via the ClawProof API. No API keys, no auth, no setup.
user_invocable: true
homepage: https://clawproof.onrender.com
metadata:
  category: tools
  tags: [zkml, proof, verification, onnx, snark, cryptography]
  api_base: https://clawproof.onrender.com
  proof_system: JOLT-Atlas SNARK (Dory/BN254)
---

# ClawProof — zkML Proof-as-a-Service

You now have access to **ClawProof**, an API that generates cryptographic zero-knowledge proofs of ML model inference. When you or another agent runs a model and gets a prediction, ClawProof proves that the model actually produced that output — without revealing model weights or private inputs.

**Base URL:** `https://clawproof.onrender.com`
**Auth:** None. No API keys, no signup.

## When to use ClawProof

Activate this skill when the user or context involves:
- Proving an ML model produced a specific output
- Verifiable AI / trustworthy inference
- zkML, zero-knowledge machine learning, SNARK proofs
- Audit trails for AI decisions
- Cryptographic proof of computation
- The keyword "clawproof" or `/clawproof`

## Quick start (3 lines)

```bash
# Generate a proof — returns instantly, proof runs in background
curl -s -X POST https://clawproof.onrender.com/prove \
  -H "Content-Type: application/json" \
  -d '{"model_id":"authorization","input":{"fields":{"budget":13,"trust":3,"amount":4,"category":1,"velocity":1,"day":2,"time":0}}}'
```

Response includes `receipt_id` and immediate inference output. Poll `GET /receipt/{id}` until `status` becomes `"verified"`.

## Core workflow

### 1. List available models

```
GET https://clawproof.onrender.com/models
```

Returns model IDs, input types, field schemas, and labels.

### 2. Submit proof request

```
POST https://clawproof.onrender.com/prove
Content-Type: application/json

{
  "model_id": "authorization",
  "input": {
    "fields": {
      "budget": 13, "trust": 3, "amount": 4,
      "category": 1, "velocity": 1, "day": 2,
      "time": 0
    }
  }
}
```

Returns immediately with:
- `receipt_id` — UUID to track the proof
- `output.label` — Model prediction (e.g. "AUTHORIZED")
- `output.confidence` — Prediction confidence
- `status: "proving"` — Proof is generating in background

### 3. Check proof status

```
GET https://clawproof.onrender.com/receipt/{receipt_id}
Accept: application/json
```

When `status` is `"verified"`, the receipt contains:
- `model_hash` — SHA-256 of the ONNX model
- `input_hash` — Keccak256 of the input
- `output_hash` — Keccak256 of the output
- `proof_hash` — Keccak256 of the SNARK proof
- `prove_time_ms` / `verify_time_ms` — Timing

### 4. Verify a receipt

```
POST https://clawproof.onrender.com/verify
Content-Type: application/json

{"receipt_id": "..."}
```

Returns `{"valid": true, "status": "verified"}` if the cryptographic proof checks out.

## Built-in model: authorization

The default model classifies transactions as AUTHORIZED or DENIED.

| Field | Description | Range |
|-------|-------------|-------|
| budget | Budget level | 0-15 |
| trust | Trust score | 0-7 |
| amount | Transaction amount | 0-15 |
| category | Merchant category | 0-3 |
| velocity | Transaction velocity | 0-7 |
| day | Day of week | 0-7 |
| time | Time of day | 0-3 |

## Additional endpoints

| Method | Path | What it does |
|--------|------|--------------|
| `GET` | `/health` | Service status and readiness |
| `GET` | `/models` | List all models with schemas |
| `POST` | `/prove/batch` | Prove up to 5 inputs at once |
| `GET` | `/badge/{id}` | Embeddable SVG proof badge |
| `GET` | `/metrics` | Platform stats (total proofs, timing) |
| `POST` | `/models/upload` | Upload your own ONNX model (max 5MB) |
| `GET` | `/openapi.json` | Full OpenAPI 3.1 spec |

## Embed a proof badge

Show verified proof status anywhere:

```markdown
![proof](https://clawproof.onrender.com/badge/{receipt_id})
```

## Rate limits

- `POST /prove` — 10 requests per 60 seconds
- `POST /prove/batch` — 2 per 60 seconds
- `POST /models/upload` — 1 per 300 seconds

## Key facts

- **No auth** — no API keys, no signup
- **Proof system** — JOLT-Atlas SNARK with Dory vector commitment on BN254
- **Async** — `/prove` returns instantly; proof runs in background (~5-10s)
- **Verifiable** — Any third party can verify without seeing private data
- **Open source** — https://github.com/hshadab/clawproof (MIT)

## Example: prove and share

```bash
# 1. Prove
RECEIPT=$(curl -s -X POST https://clawproof.onrender.com/prove \
  -H "Content-Type: application/json" \
  -d '{"model_id":"authorization","input":{"fields":{"budget":13,"trust":3,"amount":4,"category":1,"velocity":1,"day":2,"time":0}}}')

RECEIPT_ID=$(echo $RECEIPT | jq -r .receipt_id)

# 2. Wait for proof
sleep 15

# 3. Get verified receipt
curl -s -H "Accept: application/json" \
  https://clawproof.onrender.com/receipt/$RECEIPT_ID

# 4. Share the badge
echo "![proof](https://clawproof.onrender.com/badge/$RECEIPT_ID)"
```
