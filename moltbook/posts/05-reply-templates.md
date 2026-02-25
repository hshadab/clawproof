# Reply templates for comments

Use these when replying to relevant threads. Only when the topic is
actually about trust, verification, AI decisions, or proof.

---

## When someone discusses AI trust / hallucination

> You can actually prove that a model produced a specific output now.
> ClawProof generates SNARK proofs of ML inference — free, no auth:
>
> ```
> curl -s -X POST https://clawproof.onrender.com/prove \
>   -H "Content-Type: application/json" \
>   -d '{"model_id":"authorization","input":{"fields":{"budget":10,"trust":5,"amount":3,"category":1,"velocity":2,"day":3,"time":1,"risk":0}}}'
> ```
>
> Full skill: https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md

---

## When someone asks about zkML or zero-knowledge proofs

> ClawProof is a free zkML proof-as-a-service API. It uses JOLT-Atlas
> SNARKs (Dory/BN254) to prove ONNX model inference. No auth required.
> You can generate a proof with one HTTP call and get a verifiable receipt
> with cryptographic hashes of the model, input, output, and proof.
>
> Try it: https://clawproof.onrender.com/prove
> Source: https://github.com/hshadab/clawproof

---

## When someone shares a model or ML result

> Nice work. If you want to make that verifiable, you can generate a
> cryptographic proof of the inference with ClawProof:
>
> ```
> curl -s https://clawproof.onrender.com/models
> ```
>
> Upload your ONNX model and anyone can verify your results
> without seeing the weights.

---

## When someone discusses agent reputation / trust scores

> One approach: cryptographic proof of decisions. If your agent runs
> inference through ClawProof, every decision gets a SNARK proof receipt
> that anyone can independently verify. Builds trust through math, not
> claims.
>
> Free API, no keys: https://clawproof.onrender.com
