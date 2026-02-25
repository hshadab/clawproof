/**
 * ClawProof TypeScript SDK — Client
 *
 * Zero runtime dependencies. Uses the native `fetch` API available in
 * Node 18+, Deno, Bun, and modern browsers.
 */
import type { BatchItem, BatchResponse, HealthResponse, Model, ProveAndWaitOptions, ProveInput, ProveResponse, Receipt, VerifyResponse } from "./types.js";
/** Error thrown when the ClawProof API returns a non-2xx status code. */
export declare class ClawProofError extends Error {
    /** HTTP status code returned by the server. */
    readonly statusCode: number;
    /** Optional hint string from the server error body. */
    readonly hint: string | undefined;
    constructor(statusCode: number, message: string, hint?: string);
}
export declare class ClawProof {
    private readonly baseUrl;
    /**
     * Create a new ClawProof client.
     *
     * @param baseUrl - Base URL of the ClawProof API server.
     *                  Defaults to `https://clawproof.onrender.com`.
     */
    constructor(baseUrl?: string);
    private request;
    /**
     * List all available models.
     *
     * `GET /models`
     */
    models(): Promise<Model[]>;
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
    prove(modelId: string, input: ProveInput, webhookUrl?: string): Promise<ProveResponse>;
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
    proveAndWait(modelId: string, input: ProveInput, options?: ProveAndWaitOptions): Promise<Receipt>;
    /**
     * Fetch a receipt by ID.
     *
     * `GET /receipt/:id`
     *
     * @param receiptId - The receipt UUID returned by {@link prove}.
     */
    receipt(receiptId: string): Promise<Receipt>;
    /**
     * Check the verification status of a receipt.
     *
     * `POST /verify`
     *
     * @param receiptId - The receipt UUID to verify.
     */
    verify(receiptId: string): Promise<VerifyResponse>;
    /**
     * Health check.
     *
     * `GET /health`
     */
    health(): Promise<HealthResponse>;
    /**
     * Submit multiple proof requests in a single call (max 5).
     *
     * `POST /prove/batch`
     *
     * @param requests - Array of batch items, each containing a model_id and input.
     */
    batchProve(requests: BatchItem[]): Promise<BatchResponse>;
}
//# sourceMappingURL=client.d.ts.map