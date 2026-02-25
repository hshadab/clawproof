/**
 * ClawProof TypeScript SDK — Type definitions
 *
 * Mirrors the Rust server types defined in src/receipt.rs, src/models.rs,
 * src/handlers/prove.rs, src/handlers/verify.rs, src/handlers/health.rs,
 * and src/handlers/batch.rs.
 */
/** Schema for a single structured-input field. */
export interface FieldSchema {
    name: string;
    description: string;
    min: number;
    max: number;
}
/** Descriptor returned by GET /models. */
export interface Model {
    id: string;
    name: string;
    description: string;
    input_type: "text" | "structured_fields" | "raw";
    input_dim: number;
    input_shape: number[];
    labels: string[];
    trace_length: number;
    fields?: FieldSchema[];
}
/** Union input payload sent with a prove request. */
export interface ProveInput {
    text?: string;
    fields?: Record<string, number>;
    raw?: number[];
}
/** The inference result embedded in receipts and prove responses. */
export interface InferenceOutput {
    raw_output: number[];
    predicted_class: number;
    label: string;
    confidence: number;
}
/** Full receipt returned by GET /receipt/:id. */
export interface Receipt {
    id: string;
    model_id: string;
    model_name: string;
    status: "proving" | "verified" | "failed";
    created_at: string;
    completed_at: string | null;
    model_hash: string;
    input_hash: string;
    output_hash: string;
    output: InferenceOutput;
    proof_hash: string | null;
    proof_size: number | null;
    prove_time_ms: number | null;
    verify_time_ms: number | null;
    error: string | null;
}
/** Response from POST /prove. */
export interface ProveResponse {
    receipt_id: string;
    receipt_url: string;
    model_id: string;
    output: InferenceOutput;
    status: string;
}
/** Response from POST /verify. */
export interface VerifyResponse {
    valid: boolean;
    receipt_id: string;
    status: string;
}
/** Response from GET /health. */
export interface HealthResponse {
    status: string;
    version: string;
    proof_system: string;
    models_loaded: number;
    models_total: number;
    ready: boolean;
}
/** A single item in a batch prove request. */
export interface BatchItem {
    model_id: string;
    input: ProveInput;
    webhook_url?: string;
}
/** Response from POST /prove/batch. */
export interface BatchResponse {
    receipts: ProveResponse[];
}
/** Error body returned by the API on 4xx / 5xx. */
export interface ErrorResponse {
    error: string;
    hint?: string;
}
/** Options for the polling-based proveAndWait helper. */
export interface ProveAndWaitOptions {
    /** Maximum time to wait for the proof to complete (ms). Default: 600_000 (10 min). */
    timeoutMs?: number;
    /** Interval between polling requests (ms). Default: 3_000. */
    pollIntervalMs?: number;
    /** Optional webhook URL forwarded to the prove call. */
    webhookUrl?: string;
}
//# sourceMappingURL=types.d.ts.map