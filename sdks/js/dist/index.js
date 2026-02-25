/**
 * ClawProof TypeScript SDK
 *
 * Usage:
 *
 * ```ts
 * import { ClawProof } from "clawproof";
 *
 * const pg = new ClawProof();               // default: https://clawproof.onrender.com
 * const models = await pg.models();
 * const receipt = await pg.proveAndWait("authorization", {
 *   fields: { budget: 10, trust: 5, amount: 8, category: 1, velocity: 3, day: 2, time: 1, risk: 0 },
 * });
 * console.log(receipt.output.label);        // "AUTHORIZED" | "DENIED"
 * ```
 */
// Re-export the client class and its custom error.
export { ClawProof, ClawProofError } from "./client.js";
//# sourceMappingURL=index.js.map