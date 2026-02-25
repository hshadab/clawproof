pub fn render() -> String {
    r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>clawproof</title>
    <style>
        *, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }

        :root {
            --bg: #ffffff;
            --bg-secondary: #f7f8fa;
            --bg-tertiary: #eef0f4;
            --border: #d8dce3;
            --border-light: #e8ebf0;
            --text-primary: #111827;
            --text-secondary: #4b5563;
            --text-tertiary: #9ca3af;
            --accent: #111827;
            --accent-hover: #374151;
            --link: #2563eb;
            --green: #16a34a;
            --green-bg: #f0fdf4;
            --green-border: #bbf7d0;
            --amber: #d97706;
            --amber-bg: #fffbeb;
            --amber-border: #fde68a;
            --red: #dc2626;
            --red-bg: #fef2f2;
            --red-border: #fecaca;
            --mono: 'SF Mono', 'Fira Code', 'JetBrains Mono', 'Cascadia Code', Menlo, monospace;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Inter', system-ui, sans-serif;
            background: var(--bg);
            color: var(--text-primary);
            min-height: 100vh;
            -webkit-font-smoothing: antialiased;
        }

        .page { max-width: 560px; margin: 0 auto; padding: 3rem 1.25rem 4rem; }

        .header { margin-bottom: 2.5rem; }
        .wordmark { font-size: 1.125rem; font-weight: 600; letter-spacing: -0.01em; color: var(--text-primary); }
        .wordmark span { color: var(--text-tertiary); font-weight: 400; }
        .tagline { color: var(--text-tertiary); font-size: 0.8125rem; margin-top: 0.25rem; line-height: 1.5; }

        .form-group { margin-bottom: 1.25rem; }
        .form-label {
            display: block; font-size: 0.8125rem; font-weight: 500; color: var(--text-secondary);
            margin-bottom: 0.375rem;
        }

        select, textarea, input[type="number"] {
            width: 100%; padding: 0.5rem 0.625rem; background: var(--bg); border: 1px solid var(--border);
            border-radius: 6px; color: var(--text-primary); font-size: 0.875rem; font-family: inherit;
            outline: none; transition: border-color 0.15s, box-shadow 0.15s; line-height: 1.5;
        }
        select:focus, textarea:focus, input[type="number"]:focus {
            border-color: var(--accent); box-shadow: 0 0 0 3px rgba(17, 24, 39, 0.08);
        }
        textarea { resize: vertical; min-height: 88px; }
        select { cursor: pointer; }

        .model-desc {
            color: var(--text-tertiary); font-size: 0.8125rem; margin-top: 0.375rem; line-height: 1.5;
        }

        .fields-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem; }
        .field-item .form-label { font-size: 0.75rem; }
        .field-range { color: var(--text-tertiary); font-weight: 400; }
        .field-item input { padding: 0.4375rem 0.5rem; font-size: 0.8125rem; }

        .submit-btn {
            width: 100%; padding: 0.625rem; background: var(--accent); color: #fff; border: none;
            border-radius: 6px; font-size: 0.875rem; font-weight: 500; cursor: pointer;
            transition: background 0.15s; margin-top: 0.25rem; line-height: 1.5;
        }
        .submit-btn:hover { background: var(--accent-hover); }
        .submit-btn:disabled { background: var(--bg-tertiary); color: var(--text-tertiary); cursor: not-allowed; }

        .error-msg { color: var(--red); font-size: 0.8125rem; text-align: center; margin-top: 0.75rem; }

        #result { margin-top: 1.5rem; display: none; }
        .result-card { border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
        .result-top { padding: 1.25rem 1.25rem 1rem; text-align: center; }
        .result-prediction {
            font-size: 1.25rem; font-weight: 600; color: var(--text-primary); letter-spacing: -0.01em;
        }
        .result-confidence { color: var(--text-tertiary); font-size: 0.8125rem; margin-top: 0.125rem; }
        .result-bottom {
            display: flex; align-items: center; justify-content: space-between;
            padding: 0.75rem 1.25rem; background: var(--bg-secondary); border-top: 1px solid var(--border-light);
        }
        .result-status { display: flex; align-items: center; gap: 0.375rem; }

        .status-dot {
            width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0;
        }
        .status-dot.proving { background: var(--amber); animation: pulse 1.5s ease-in-out infinite; }
        .status-dot.verified { background: var(--green); }
        .status-dot.failed { background: var(--red); }
        @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }

        .status-text { font-size: 0.8125rem; font-weight: 500; color: var(--text-secondary); }
        .status-text.proving { color: var(--amber); }
        .status-text.verified { color: var(--green); }
        .status-text.failed { color: var(--red); }

        .result-link {
            font-family: var(--mono); font-size: 0.6875rem; color: var(--link); text-decoration: none;
            word-break: break-all;
        }
        .result-link:hover { text-decoration: underline; }

        /* Collapsible sections */
        .info-sections { margin-top: 3rem; }
        .info-section { border: 1px solid var(--border-light); border-radius: 8px; margin-bottom: 0.75rem; overflow: hidden; }
        .info-toggle {
            width: 100%; display: flex; align-items: center; justify-content: space-between;
            padding: 0.75rem 1rem; background: var(--bg-secondary); border: none;
            font-size: 0.8125rem; font-weight: 600; color: var(--text-primary);
            cursor: pointer; font-family: inherit; text-align: left;
        }
        .info-toggle:hover { background: var(--bg-tertiary); }
        .info-toggle .arrow { color: var(--text-tertiary); transition: transform 0.2s; font-size: 0.75rem; }
        .info-section.open .arrow { transform: rotate(90deg); }
        .info-content { display: none; padding: 1rem; font-size: 0.8125rem; line-height: 1.7; color: var(--text-secondary); }
        .info-section.open .info-content { display: block; }
        .info-content code {
            font-family: var(--mono); font-size: 0.75rem; background: var(--bg-tertiary);
            padding: 0.125rem 0.375rem; border-radius: 3px; color: var(--text-primary);
        }
        .info-content pre {
            font-family: var(--mono); font-size: 0.6875rem; background: var(--bg-secondary);
            border: 1px solid var(--border-light); border-radius: 6px; padding: 0.75rem;
            overflow-x: auto; margin: 0.5rem 0; line-height: 1.6; color: var(--text-primary);
        }
        .info-content h4 { color: var(--text-primary); font-size: 0.8125rem; margin: 0.75rem 0 0.25rem; }
        .info-content h4:first-child { margin-top: 0; }

        .footer {
            text-align: center; margin-top: 3rem; padding-top: 1.5rem;
            border-top: 1px solid var(--border-light); color: var(--text-tertiary); font-size: 0.75rem;
        }
        .footer a { color: var(--text-tertiary); text-decoration: none; }
        .footer a:hover { color: var(--text-secondary); }
    </style>
</head>
<body>
    <div class="page">
        <div class="header">
            <div class="wordmark">clawproof <span>/ zkML</span></div>
            <p class="tagline">Cryptographic proof receipts for AI-driven transaction decisions. Built for agentic commerce and AI security.</p>
        </div>

        <div class="form-group">
            <label class="form-label" for="model-select">Model</label>
            <select id="model-select"><option value="">Loading...</option></select>
            <p class="model-desc" id="model-desc"></p>
        </div>

        <div id="input-area"></div>

        <button class="submit-btn" id="prove-btn" disabled>Generate proof</button>

        <div id="error" class="error-msg" aria-live="assertive"></div>

        <div id="result" aria-live="polite">
            <div class="result-card">
                <div class="result-top">
                    <div class="result-prediction" id="res-label"></div>
                    <div class="result-confidence" id="res-confidence"></div>
                </div>
                <div class="result-bottom">
                    <div class="result-status">
                        <span class="status-dot" id="res-dot" aria-label="Proof status"></span>
                        <span class="status-text" id="res-status-text"></span>
                    </div>
                    <a class="result-link" id="res-link" target="_blank"></a>
                </div>
            </div>
        </div>

        <div class="info-sections">
            <div class="info-section">
                <button class="info-toggle" onclick="toggleSection(this)">
                    For Agents <span class="arrow">&#9654;</span>
                </button>
                <div class="info-content">
                    <h4>MCP (Claude Desktop)</h4>
                    <p>Add to your Claude Desktop config:</p>
<pre>{
  "mcpServers": {
    "clawproof": {
      "command": "clawproof-mcp",
      "env": {
        "CLAWPROOF_URL": "https://clawproof.onrender.com"
      }
    }
  }
}</pre>
                    <h4>OpenAPI</h4>
                    <p>Machine-readable API spec: <a href="/openapi.json" style="color:var(--link)">/openapi.json</a></p>

                    <h4>Python SDK</h4>
<pre>pip install clawproof

from clawproof import ClawProof
pg = ClawProof()
receipt = pg.prove_and_wait("authorization",
    fields={"budget": 5, "trust": 3, "amount": 8,
            "category": 1, "velocity": 2, "day": 3,
            "time": 1, "risk": 0})
print(receipt.output.label)</pre>

                    <h4>JavaScript SDK</h4>
<pre>npm install clawproof

import { ClawProof } from "clawproof";
const pg = new ClawProof();
const receipt = await pg.proveAndWait("authorization", {
  fields: { budget: 5, trust: 3, amount: 8,
            category: 1, velocity: 2, day: 3,
            time: 1, risk: 0 }
});</pre>
                </div>
            </div>

            <div class="info-section">
                <button class="info-toggle" onclick="toggleSection(this)">
                    For Compliance <span class="arrow">&#9654;</span>
                </button>
                <div class="info-content">
                    <h4>What zkML proves</h4>
                    <p>Each proof receipt cryptographically attests that a specific ML model produced a specific authorization decision for a specific transaction. When an AI agent approves or denies a transaction, the JOLT-Atlas SNARK proves the inference was computed correctly &mdash; creating an auditable, tamper-proof record without revealing model weights.</p>

                    <h4>What the receipt contains</h4>
                    <p>
                        <strong>Model hash</strong> &mdash; Keccak256 of the ONNX model file<br>
                        <strong>Input hash</strong> &mdash; Keccak256 of the encoded input tensor<br>
                        <strong>Output hash</strong> &mdash; Keccak256 of the raw model output<br>
                        <strong>Proof hash</strong> &mdash; Keccak256 of the serialized SNARK proof<br>
                        <strong>Timing</strong> &mdash; Proof generation and verification times
                    </p>

                    <h4>How to verify</h4>
                    <p>Call <code>POST /verify</code> with the receipt ID. The server checks the SNARK proof against the verifier preprocessing. Receipts are stored persistently in SQLite and survive server restarts.</p>

                    <h4>Export</h4>
                    <p>Append <code>?format=jsonld</code> to any receipt URL for a JSON-LD document conforming to <a href="https://schema.org" style="color:var(--link)">schema.org</a> DigitalDocument type.</p>
                </div>
            </div>

            <div class="info-section">
                <button class="info-toggle" onclick="toggleSection(this)">
                    API Reference <span class="arrow">&#9654;</span>
                </button>
                <div class="info-content">
                    <h4>POST /prove</h4>
<pre>curl -X POST https://clawproof.onrender.com/prove \
  -H "Content-Type: application/json" \
  -d '{"model_id":"authorization","input":{"fields":{"budget":5,"trust":3,"amount":8,"category":1,"velocity":2,"day":3,"time":1,"risk":0}}}'</pre>

                    <h4>POST /prove/batch</h4>
<pre>curl -X POST https://clawproof.onrender.com/prove/batch \
  -H "Content-Type: application/json" \
  -d '{"requests":[{"model_id":"authorization","input":{"fields":{"budget":5,"trust":3,"amount":8,"category":1,"velocity":2,"day":3,"time":1,"risk":0}}}]}'</pre>

                    <h4>GET /receipt/{id}</h4>
<pre>curl https://clawproof.onrender.com/receipt/{id} \
  -H "Accept: application/json"</pre>

                    <h4>POST /verify</h4>
<pre>curl -X POST https://clawproof.onrender.com/verify \
  -H "Content-Type: application/json" \
  -d '{"receipt_id":"..."}'</pre>

                    <h4>GET /metrics</h4>
<pre>curl https://clawproof.onrender.com/metrics</pre>

                    <h4>GET /badge/{receipt_id}</h4>
                    <p>Embed in markdown: <code>![proof](https://clawproof.onrender.com/badge/{receipt_id})</code></p>

                    <h4>POST /models/upload</h4>
<pre>curl -X POST https://clawproof.onrender.com/models/upload \
  -F "onnx_file=@model.onnx" \
  -F "name=My Model" \
  -F "input_dim=64" \
  -F 'labels=["class_a","class_b"]' \
  -F "trace_length=16384"</pre>

                    <h4>All endpoints</h4>
                    <p>
                        <code>GET /health</code> &middot;
                        <code>GET /models</code> &middot;
                        <code>POST /prove</code> &middot;
                        <code>POST /prove/batch</code> &middot;
                        <code>GET /receipt/{id}</code> &middot;
                        <code>POST /verify</code> &middot;
                        <code>GET /metrics</code> &middot;
                        <code>GET /badge/{id}</code> &middot;
                        <code>POST /models/upload</code> &middot;
                        <code>POST /convert</code> &middot;
                        <code>GET /openapi.json</code>
                    </p>
                </div>
            </div>
        </div>

        <div class="footer">
            <a href="https://github.com/ICME-Lab/jolt-atlas" target="_blank">JOLT-Atlas</a>
        </div>
    </div>

<script>
let models = [];
let pollTimer = null;

function toggleSection(btn) {
    btn.parentElement.classList.toggle('open');
}

async function init() {
    try {
        const res = await fetch('/models');
        models = await res.json();
        const sel = document.getElementById('model-select');
        sel.innerHTML = models.map(m => `<option value="${m.id}">${m.name}</option>`).join('');
        sel.addEventListener('change', renderInputs);
        renderInputs();
        document.getElementById('prove-btn').disabled = false;
    } catch(e) {
        document.getElementById('error').textContent = 'Failed to load models.';
    }
}

function renderInputs() {
    const sel = document.getElementById('model-select');
    const m = models.find(x => x.id === sel.value);
    if (!m) return;

    document.getElementById('model-desc').textContent = m.description;
    const area = document.getElementById('input-area');

    if (m.input_type === 'text') {
        area.innerHTML = `<div class="form-group"><label class="form-label" for="text-input">Input text</label><textarea id="text-input" placeholder="Enter text to classify..." maxlength="10000"></textarea></div>`;
    } else if (m.fields) {
        let html = '<div class="form-group"><label class="form-label">Parameters</label><div class="fields-grid">';
        for (const f of m.fields) {
            html += `<div class="field-item"><label class="form-label">${f.name} <span class="field-range">${f.min}\u2013${f.max}</span></label><input type="number" data-field="${f.name}" min="${f.min}" max="${f.max}" value="${f.min}"></div>`;
        }
        html += '</div></div>';
        area.innerHTML = html;
    } else if (m.input_type === 'raw') {
        area.innerHTML = `<div class="form-group"><label class="form-label" for="raw-input">Raw input vector (JSON array of ${m.input_dim} integers)</label><textarea id="raw-input" placeholder="[0, 1, 2, ...]" rows="3"></textarea></div>`;
    }
}

function setStatus(status) {
    const dot = document.getElementById('res-dot');
    const text = document.getElementById('res-status-text');
    dot.className = 'status-dot ' + status;
    text.className = 'status-text ' + status;
    if (status === 'proving') text.textContent = 'Proving';
    else if (status === 'verified') text.textContent = 'Verified';
    else if (status === 'failed') text.textContent = 'Failed';
}

async function prove() {
    const btn = document.getElementById('prove-btn');
    const errEl = document.getElementById('error');
    const resEl = document.getElementById('result');
    errEl.textContent = '';
    resEl.style.display = 'none';
    btn.disabled = true;
    btn.textContent = 'Generating...';
    if (pollTimer) clearInterval(pollTimer);

    const sel = document.getElementById('model-select');
    const m = models.find(x => x.id === sel.value);
    if (!m) return;

    let body = { model_id: m.id, input: {} };

    if (m.input_type === 'text') {
        const text = document.getElementById('text-input')?.value || '';
        if (!text.trim()) {
            errEl.textContent = 'Please enter some text.';
            btn.disabled = false;
            btn.textContent = 'Generate proof';
            return;
        }
        body.input.text = text;
    } else if (m.fields) {
        body.input.fields = {};
        for (const f of m.fields) {
            const el = document.querySelector(`[data-field="${f.name}"]`);
            const val = parseInt(el?.value || '0');
            if (val < f.min || val > f.max) {
                errEl.textContent = `${f.name} must be between ${f.min} and ${f.max}.`;
                btn.disabled = false;
                btn.textContent = 'Generate proof';
                return;
            }
            body.input.fields[f.name] = val;
        }
    } else if (m.input_type === 'raw') {
        const rawText = document.getElementById('raw-input')?.value || '';
        try {
            body.input.raw = JSON.parse(rawText);
        } catch(e) {
            errEl.textContent = 'Invalid JSON array for raw input.';
            btn.disabled = false;
            btn.textContent = 'Generate proof';
            return;
        }
    }

    try {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 30000);
        const res = await fetch('/prove', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
            signal: controller.signal,
        });
        clearTimeout(timeout);
        if (!res.ok) {
            const err = await res.json();
            throw new Error(err.error || 'Request failed');
        }
        const data = await res.json();

        document.getElementById('res-label').textContent = data.output.label;
        document.getElementById('res-confidence').textContent =
            (data.output.confidence * 100).toFixed(1) + '% confidence';
        setStatus('proving');
        const link = document.getElementById('res-link');
        link.href = data.receipt_url;
        link.textContent = 'View receipt';
        resEl.style.display = 'block';

        pollTimer = setInterval(async () => {
            try {
                const r = await fetch(`/receipt/${data.receipt_id}`, {
                    headers: { Accept: 'application/json' },
                });
                const receipt = await r.json();
                if (receipt.status === 'verified') {
                    clearInterval(pollTimer);
                    setStatus('verified');
                } else if (receipt.status === 'failed') {
                    clearInterval(pollTimer);
                    setStatus('failed');
                }
            } catch (e) {
                console.error('Polling failed:', e);
                clearInterval(pollTimer);
                setStatus('failed');
                document.getElementById('res-status-text').textContent = 'Connection lost';
            }
        }, 3000);
    } catch (e) {
        errEl.textContent = e.message;
    }

    btn.disabled = false;
    btn.textContent = 'Generate proof';
}

document.getElementById('prove-btn').addEventListener('click', prove);
init();
</script>
</body>
</html>"##.to_string()
}
