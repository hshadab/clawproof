/**
 * ClawProof TypeScript SDK — Client
 *
 * Zero runtime dependencies. Uses the native `fetch` API available in
 * Node 18+, Deno, Bun, and modern browsers.
 */

import type {
  BatchItem,
  BatchResponse,
  ErrorResponse,
  HealthResponse,
  Model,
  ProveAndWaitOptions,
  ProveInput,
  ProveResponse,
  Receipt,
  VerifyResponse,
} from "./types.js";

// ---------------------------------------------------------------------------
// Custom error
// ---------------------------------------------------------------------------

/** Error thrown when the ClawProof API returns a non-2xx status code. */
export class ClawProofError extends Error {
  /** HTTP status code returned by the server. */
  public readonly statusCode: number;
  /** Optional hint string from the server error body. */
  public readonly hint: string | undefined;

  constructor(statusCode: number, message: string, hint?: string) {
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
  private readonly baseUrl: string;

  /**
   * Create a new ClawProof client.
   *
   * @param baseUrl - Base URL of the ClawProof API server.
   *                  Defaults to `https://clawproof.onrender.com`.
   */
  constructor(baseUrl: string = DEFAULT_BASE_URL) {
    // Strip trailing slash so we can safely append paths.
    this.baseUrl = baseUrl.replace(/\/+$/, "");
  }

  // -----------------------------------------------------------------------
  // Internal helpers
  // -----------------------------------------------------------------------

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;

    const headers: Record<string, string> = {
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
      let errorBody: ErrorResponse | undefined;
      try {
        errorBody = (await res.json()) as ErrorResponse;
      } catch {
        // Response body could not be parsed as JSON — fall through.
      }

      throw new ClawProofError(
        res.status,
        errorBody?.error ?? `HTTP ${res.status} ${res.statusText}`,
        errorBody?.hint,
      );
    }

    return (await res.json()) as T;
  }

  // -----------------------------------------------------------------------
  // Public API
  // -----------------------------------------------------------------------

  /**
   * List all available models.
   *
   * `GET /models`
   */
  async models(): Promise<Model[]> {
    return this.request<Model[]>("GET", "/models");
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
  async prove(
    modelId: string,
    input: ProveInput,
    webhookUrl?: string,
  ): Promise<ProveResponse> {
    return this.request<ProveResponse>("POST", "/prove", {
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
  async proveAndWait(
    modelId: string,
    input: ProveInput,
    options?: ProveAndWaitOptions,
  ): Promise<Receipt> {
    const timeoutMs = options?.timeoutMs ?? 600_000;
    const pollIntervalMs = options?.pollIntervalMs ?? 3_000;

    const proveRes = await this.prove(modelId, input, options?.webhookUrl);

    const deadline = Date.now() + timeoutMs;

    while (Date.now() < deadline) {
      const r = await this.receipt(proveRes.receipt_id);

      if (r.status === "verified" || r.status === "failed") {
        return r;
      }

      // Wait before next poll.
      await new Promise<void>((resolve) => setTimeout(resolve, pollIntervalMs));
    }

    throw new ClawProofError(
      408,
      `Proof did not complete within ${timeoutMs}ms (receipt ${proveRes.receipt_id})`,
    );
  }

  /**
   * Fetch a receipt by ID.
   *
   * `GET /receipt/:id`
   *
   * @param receiptId - The receipt UUID returned by {@link prove}.
   */
  async receipt(receiptId: string): Promise<Receipt> {
    return this.request<Receipt>("GET", `/receipt/${encodeURIComponent(receiptId)}`);
  }

  /**
   * Check the verification status of a receipt.
   *
   * `POST /verify`
   *
   * @param receiptId - The receipt UUID to verify.
   */
  async verify(receiptId: string): Promise<VerifyResponse> {
    return this.request<VerifyResponse>("POST", "/verify", {
      receipt_id: receiptId,
    });
  }

  /**
   * Health check.
   *
   * `GET /health`
   */
  async health(): Promise<HealthResponse> {
    return this.request<HealthResponse>("GET", "/health");
  }

  /**
   * Submit multiple proof requests in a single call (max 5).
   *
   * `POST /prove/batch`
   *
   * @param requests - Array of batch items, each containing a model_id and input.
   */
  async batchProve(requests: BatchItem[]): Promise<BatchResponse> {
    return this.request<BatchResponse>("POST", "/prove/batch", {
      requests,
    });
  }
}
