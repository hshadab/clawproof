/**
 * ClawProof TypeScript SDK — Client
 *
 * Zero runtime dependencies. Uses the native `fetch` API available in
 * Node 18+, Deno, Bun, and modern browsers.
 */
// ---------------------------------------------------------------------------
// Custom error
// ---------------------------------------------------------------------------
/** Error thrown when the ClawProof API returns a non-2xx status code. */
export class ClawProofError extends Error {
    constructor(statusCode, message, hint) {
        super(message);
        this.name = "ClawProofError";
        this.statusCode = statusCode;
        this.hint = hint;
    }
}
// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------
const DEFAULT_BASE_URL = "https://clawproof.onrender.com";
export class ClawProof {
    /**
     * Create a new ClawProof client.
     *
     * @param baseUrl - Base URL of the ClawProof API server.
     *                  Defaults to `https://clawproof.onrender.com`.
     */
    constructor(baseUrl = DEFAULT_BASE_URL) {
        // Strip trailing slash so we can safely append paths.
        this.baseUrl = baseUrl.replace(/\/+$/, "");
    }
    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------
    async request(method, path, body) {
        const url = `${this.baseUrl}${path}`;
        const headers = {
            Accept: "application/json",
        };
        if (body !== undefined) {
            headers["Content-Type"] = "application/json";
        }
        const res = await fetch(url, {
            method,
            headers,
            body: body !== undefined ? JSON.stringify(body) : undefined,
        });
        if (!res.ok) {
            let errorBody;
            try {
                errorBody = (await res.json());
            }
            catch {
                // Response body could not be parsed as JSON — fall through.
            }
            throw new ClawProofError(res.status, errorBody?.error ?? `HTTP ${res.status} ${res.statusText}`, errorBody?.hint);
        }
        return (await res.json());
    }
    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------
    /**
     * List all available models.
     *
     * `GET /models`
     */
    async models() {
        return this.request("GET", "/models");
    }
    /**
     * Submit a proof request and return immediately with a receipt stub.
     *
     * The proof is generated asynchronously on the server. Use
     * {@link receipt} or {@link proveAndWait} to retrieve the completed proof.
     *
     * `POST /prove`
     *
     * @param modelId   - ID of the model to run inference on.
     * @param input     - Input payload (text, fields, or raw vector).
     * @param webhookUrl - Optional HTTPS URL to receive a POST when the proof is ready.
     */
    async prove(modelId, input, webhookUrl) {
        return this.request("POST", "/prove", {
            model_id: modelId,
            input,
            ...(webhookUrl !== undefined ? { webhook_url: webhookUrl } : {}),
        });
    }
    /**
     * Submit a proof request and poll until it completes (or times out).
     *
     * This is a convenience wrapper around {@link prove} + {@link receipt}.
     *
     * @param modelId - ID of the model to run inference on.
     * @param input   - Input payload.
     * @param options - Polling and timeout configuration.
     * @returns The completed (verified or failed) receipt.
     */
    async proveAndWait(modelId, input, options) {
        const timeoutMs = options?.timeoutMs ?? 600000;
        const pollIntervalMs = options?.pollIntervalMs ?? 3000;
        const proveRes = await this.prove(modelId, input, options?.webhookUrl);
        const deadline = Date.now() + timeoutMs;
        while (Date.now() < deadline) {
            const r = await this.receipt(proveRes.receipt_id);
            if (r.status === "verified" || r.status === "failed") {
                return r;
            }
            // Wait before next poll.
            await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
        }
        throw new ClawProofError(408, `Proof did not complete within ${timeoutMs}ms (receipt ${proveRes.receipt_id})`);
    }
    /**
     * Fetch a receipt by ID.
     *
     * `GET /receipt/:id`
     *
     * @param receiptId - The receipt UUID returned by {@link prove}.
     */
    async receipt(receiptId) {
        return this.request("GET", `/receipt/${encodeURIComponent(receiptId)}`);
    }
    /**
     * Check the verification status of a receipt.
     *
     * `POST /verify`
     *
     * @param receiptId - The receipt UUID to verify.
     */
    async verify(receiptId) {
        return this.request("POST", "/verify", {
            receipt_id: receiptId,
        });
    }
    /**
     * Health check.
     *
     * `GET /health`
     */
    async health() {
        return this.request("GET", "/health");
    }
    /**
     * Submit multiple proof requests in a single call (max 5).
     *
     * `POST /prove/batch`
     *
     * @param requests - Array of batch items, each containing a model_id and input.
     */
    async batchProve(requests) {
        return this.request("POST", "/prove/batch", {
            requests,
        });
    }
}
//# sourceMappingURL=client.js.map