use crate::crypto;
use crate::receipt::{ReceiptStatus, ReceiptStore};
use crate::state::PreprocessingCache;

use ark_bn254::Fr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use dashmap::DashMap;
use jolt_core::poly::commitment::dory::DoryCommitmentScheme;
use jolt_core::transcripts::KeccakTranscript;
use onnx_tracer::{model, tensor::Tensor, ProgramIO};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};
use zkml_jolt_core::jolt::JoltSNARK;

#[allow(clippy::upper_case_acronyms)]
type PCS = DoryCommitmentScheme;
type Snark = JoltSNARK<Fr, PCS, KeccakTranscript>;

pub fn prove_and_verify(
    receipt_id: String,
    receipt_store: ReceiptStore,
    preprocessing_map: Arc<DashMap<String, PreprocessingCache>>,
    model_id: String,
    models_dir: PathBuf,
    uploaded_models_dir: PathBuf,
    input_tensor: Tensor<i32>,
    webhook_url: Option<String>,
) {
    tokio::task::spawn_blocking(move || {
        let total_start = Instant::now();

        let preprocessing_ref = match preprocessing_map.get(&model_id) {
            Some(p) => p,
            None => {
                error!("[clawproof] No preprocessing found for model {}", model_id);
                receipt_store.update(&receipt_id, |r| {
                    r.status = ReceiptStatus::Failed;
                    r.error = Some("No preprocessing available".to_string());
                    r.completed_at = Some(chrono::Utc::now());
                });
                return;
            }
        };

        // --- Prove ---
        info!(
            "[clawproof] Starting proof generation for receipt {}",
            receipt_id
        );
        let prove_start = Instant::now();

        let model_path = {
            let default_path = models_dir.join(&model_id).join("network.onnx");
            if default_path.exists() {
                default_path
            } else {
                uploaded_models_dir.join(&model_id).join("network.onnx")
            }
        };
        let model_path_for_prove = model_path.clone();
        let prove_fn = || model(&model_path_for_prove);

        let (snark, program_io, _debug_info) =
            Snark::prove(&preprocessing_ref.prover, prove_fn, &input_tensor);

        let prove_time = prove_start.elapsed();
        info!(
            "[clawproof] Proof generated in {}ms for receipt {}",
            prove_time.as_millis(),
            receipt_id
        );

        // --- Serialize proof ---
        let mut proof_bytes = Vec::new();
        if let Err(e) = snark.serialize_compressed(&mut proof_bytes) {
            error!("[clawproof] Proof serialization failed: {:?}", e);
            receipt_store.update(&receipt_id, |r| {
                r.status = ReceiptStatus::Failed;
                r.error = Some("Proof generation failed".to_string());
                r.completed_at = Some(chrono::Utc::now());
            });
            return;
        }

        let proof_hash = crypto::keccak256(&proof_bytes);
        let proof_size = proof_bytes.len();

        info!(
            "[clawproof] Proof serialized: {} bytes, hash: {}...",
            proof_size,
            &proof_hash[..10]
        );

        // --- Serialize ProgramIO for verification ---
        let program_io_json = match serde_json::to_string(&program_io) {
            Ok(j) => j,
            Err(e) => {
                error!("[clawproof] ProgramIO serialization failed: {:?}", e);
                receipt_store.update(&receipt_id, |r| {
                    r.status = ReceiptStatus::Failed;
                    r.error = Some("Proof generation failed".to_string());
                    r.completed_at = Some(chrono::Utc::now());
                });
                return;
            }
        };

        // --- Verify ---
        info!(
            "[clawproof] Starting verification for receipt {}",
            receipt_id
        );
        let verify_start = Instant::now();

        let deserialized_snark: Snark =
            match Snark::deserialize_compressed(proof_bytes.as_slice()) {
                Ok(s) => s,
                Err(e) => {
                    error!(
                        "[clawproof] Proof deserialization failed: {:?}",
                        e
                    );
                    receipt_store.update(&receipt_id, |r| {
                        r.status = ReceiptStatus::Failed;
                        r.error = Some("Proof verification failed".to_string());
                        r.completed_at = Some(chrono::Utc::now());
                    });
                    return;
                }
            };

        let deserialized_io: ProgramIO = match serde_json::from_str(&program_io_json) {
            Ok(io) => io,
            Err(e) => {
                error!("[clawproof] ProgramIO deserialization failed: {:?}", e);
                receipt_store.update(&receipt_id, |r| {
                    r.status = ReceiptStatus::Failed;
                    r.error = Some("Proof verification failed".to_string());
                    r.completed_at = Some(chrono::Utc::now());
                });
                return;
            }
        };

        match deserialized_snark.verify(&preprocessing_ref.verifier, deserialized_io, None) {
            Ok(()) => {
                let verify_time = verify_start.elapsed();
                info!(
                    "[clawproof] Proof verified in {}ms for receipt {}. Total: {}ms",
                    verify_time.as_millis(),
                    receipt_id,
                    total_start.elapsed().as_millis()
                );

                receipt_store.update(&receipt_id, |r| {
                    r.status = ReceiptStatus::Verified;
                    r.proof_hash = Some(proof_hash);
                    r.proof_size = Some(proof_size);
                    r.prove_time_ms = Some(prove_time.as_millis());
                    r.verify_time_ms = Some(verify_time.as_millis());
                    r.completed_at = Some(chrono::Utc::now());
                });

                // Fire webhook if provided
                if let Some(url) = webhook_url {
                    fire_webhook(&receipt_store, &receipt_id, &url);
                }
            }
            Err(e) => {
                error!("[clawproof] Proof verification failed: {:?}", e);
                receipt_store.update(&receipt_id, |r| {
                    r.status = ReceiptStatus::Failed;
                    r.error = Some("Proof verification failed".to_string());
                    r.completed_at = Some(chrono::Utc::now());
                });

                // Fire webhook on failure too
                if let Some(url) = webhook_url {
                    fire_webhook(&receipt_store, &receipt_id, &url);
                }
            }
        }
    });
}

fn fire_webhook(receipt_store: &ReceiptStore, receipt_id: &str, url: &str) {
    if let Some(receipt) = receipt_store.get(receipt_id) {
        let url = url.to_string();
        let handle = tokio::runtime::Handle::current();
        handle.spawn(async move {
            let client = reqwest::Client::new();
            let result = client.post(&url).json(&receipt).send().await;
            match result {
                Ok(resp) => {
                    info!(
                        "[clawproof] Webhook sent to {}, status: {}",
                        url,
                        resp.status()
                    );
                }
                Err(e) => {
                    error!("[clawproof] Webhook failed: {:?}, retrying in 5s", e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    if let Err(e2) = client.post(&url).json(&receipt).send().await {
                        error!("[clawproof] Webhook retry failed: {:?}", e2);
                    }
                }
            }
        });
    }
}
