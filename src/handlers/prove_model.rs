use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use tracing::{error, info};

use super::prove::{ErrorResponse, ProveInput, run_single_prove};
use crate::models::{InputType, ModelDescriptor};
use crate::state::{AppState, PreprocessingCache};

use ark_bn254::Fr;
use jolt_core::poly::commitment::dory::DoryCommitmentScheme;
use jolt_core::transcripts::KeccakTranscript;
use onnx_tracer::model;
use zkml_jolt_core::jolt::JoltSNARK;

#[allow(clippy::upper_case_acronyms)]
type PCS = DoryCommitmentScheme;
type Snark = JoltSNARK<Fr, PCS, KeccakTranscript>;

#[derive(Serialize)]
pub struct ProveModelResponse {
    pub receipt_id: String,
    pub receipt_url: String,
    pub model_id: String,
    pub output: crate::receipt::InferenceOutput,
    pub status: String,
}

/// Unified endpoint: upload a model file + input, get a proof back.
///
/// Accepts multipart form with:
///   - `onnx_file` or `model_file`: the model (ONNX, or .pt/.pkl/.pb if converter is available)
///   - `source_format` (optional): "onnx" (default), "pytorch", "sklearn", "tensorflow"
///   - `input_raw`: JSON array of i32 (the raw input vector)
///   - `input_dim`: integer, required
///   - `labels`: JSON array of strings (optional, defaults to ["class_0", "class_1"])
///   - `trace_length`: integer (optional, defaults to 16384)
///   - `name`: model name (optional)
///   - `webhook_url`: HTTPS callback URL (optional)
pub async fn prove_model(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ProveModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut model_bytes: Option<Vec<u8>> = None;
    let mut source_format = "onnx".to_string();
    let mut input_raw: Option<Vec<i32>> = None;
    let mut input_dim: usize = 0;
    let mut labels: Vec<String> = Vec::new();
    let mut trace_length: usize = 1 << 14;
    let mut name = "uploaded".to_string();
    let mut webhook_url: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name: String = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "onnx_file" | "model_file" => {
                let bytes = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Failed to read model file: {}", e),
                            hint: None,
                        }),
                    )
                })?;
                if bytes.len() > 5 * 1024 * 1024 {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(ErrorResponse {
                            error: "Model file exceeds 5MB limit".to_string(),
                            hint: None,
                        }),
                    ));
                }
                model_bytes = Some(bytes.to_vec());
            }
            "source_format" => {
                source_format = field.text().await.unwrap_or_default().to_lowercase();
            }
            "input_raw" => {
                let text = field.text().await.unwrap_or_default();
                input_raw = Some(serde_json::from_str(&text).map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Invalid input_raw JSON: {}", e),
                            hint: Some("Provide a JSON array of integers, e.g. [0, 1, 2, ...]".to_string()),
                        }),
                    )
                })?);
            }
            "input_dim" => {
                let text = field.text().await.unwrap_or_default();
                input_dim = text.parse().unwrap_or(0);
            }
            "labels" => {
                let text = field.text().await.unwrap_or_default();
                labels = serde_json::from_str(&text).unwrap_or_default();
            }
            "trace_length" => {
                let text = field.text().await.unwrap_or_default();
                trace_length = text.parse().unwrap_or(1 << 14);
            }
            "name" => {
                name = field.text().await.unwrap_or_default();
            }
            "webhook_url" => {
                webhook_url = Some(field.text().await.unwrap_or_default());
            }
            _ => {}
        }
    }

    let model_bytes = model_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing model file (onnx_file or model_file field)".to_string(),
                hint: Some("Upload an ONNX model as multipart field 'onnx_file'".to_string()),
            }),
        )
    })?;

    let input_raw = input_raw.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing input_raw field".to_string(),
                hint: Some("Provide input as JSON array: input_raw=[0, 1, 2, ...]".to_string()),
            }),
        )
    })?;

    if input_dim == 0 {
        input_dim = input_raw.len();
    }

    if labels.is_empty() {
        labels = vec!["class_0".to_string(), "class_1".to_string()];
    }

    // Convert to ONNX if needed
    let onnx_bytes = if source_format == "onnx" {
        model_bytes
    } else {
        let converter_url = state.config.converter_url.as_ref().ok_or_else(|| {
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(ErrorResponse {
                    error: format!("Conversion from '{}' requires the converter sidecar", source_format),
                    hint: Some("Upload an ONNX file directly, or wait for the converter service".to_string()),
                }),
            )
        })?;

        let client = reqwest::Client::new();
        let url = format!("{}/convert", converter_url);
        let part = reqwest::multipart::Part::bytes(model_bytes)
            .file_name("model")
            .mime_str("application/octet-stream")
            .unwrap();
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("source_format", source_format.clone());

        let resp = client.post(&url).multipart(form).send().await.map_err(|e| {
            error!("[clawproof] Converter proxy failed: {:?}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: "Converter service unavailable".to_string(),
                    hint: Some("The model conversion sidecar is not responding".to_string()),
                }),
            )
        })?;

        if !resp.status().is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: format!("Model conversion failed: {}", detail),
                    hint: None,
                }),
            ));
        }

        resp.bytes().await.map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("Failed to read converted model: {}", e),
                    hint: None,
                }),
            )
        })?.to_vec()
    };

    // Save ONNX to temp model directory
    let model_id = format!(
        "{}_{}",
        name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>(),
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    let model_dir = state.config.uploaded_models_dir.join(&model_id);
    std::fs::create_dir_all(&model_dir).map_err(|e| {
        error!("[clawproof] Failed to create model dir: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to save model".to_string(),
                hint: None,
            }),
        )
    })?;

    let onnx_path = model_dir.join("network.onnx");
    std::fs::write(&onnx_path, &onnx_bytes).map_err(|e| {
        error!("[clawproof] Failed to write ONNX file: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to save model".to_string(),
                hint: None,
            }),
        )
    })?;

    // Quick magic-byte check before attempting to load
    if onnx_bytes.len() < 4 || &onnx_bytes[..4] != b"\x08\x03\x12\x04" && &onnx_bytes[..2] != b"\x08\x03" {
        // ONNX protobuf files start with field 1 (ir_version) varint tag 0x08
        // Do a best-effort check — if it doesn't even look like protobuf, reject early
        if onnx_bytes.len() < 2 || onnx_bytes[0] != 0x08 {
            let _ = std::fs::remove_dir_all(&model_dir);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "File does not appear to be an ONNX model".to_string(),
                    hint: Some("Upload a valid .onnx file (ONNX protobuf format)".to_string()),
                }),
            ));
        }
    }

    // Validate ONNX by loading
    let onnx_path_clone = onnx_path.clone();
    let validation = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = model(&onnx_path_clone);
        }))
    }).await;

    match validation {
        Ok(Ok(())) => {}
        _ => {
            let _ = std::fs::remove_dir_all(&model_dir);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid ONNX model — failed to load".to_string(),
                    hint: Some("Ensure the file is a valid ONNX model with supported operations".to_string()),
                }),
            ));
        }
    }

    // Save model.toml
    let toml_content = format!(
        r#"id = "{model_id}"
name = "{name}"
description = "Uploaded via /prove/model"
input_type = "raw"
input_dim = {input_dim}
input_shape = [1, {input_dim}]
labels = [{labels_str}]
trace_length = {trace_length}
"#,
        model_id = model_id,
        name = name,
        input_dim = input_dim,
        labels_str = labels.iter().map(|l| format!("\"{}\"", l)).collect::<Vec<_>>().join(", "),
        trace_length = trace_length,
    );
    let _ = std::fs::write(model_dir.join("model.toml"), &toml_content);

    // Register in model registry
    let descriptor = ModelDescriptor {
        id: model_id.clone(),
        name: name.clone(),
        description: "Uploaded via /prove/model".to_string(),
        input_type: InputType::Raw,
        input_dim,
        input_shape: vec![1, input_dim],
        labels,
        trace_length,
        fields: None,
    };

    {
        let mut registry = state.registry.write().unwrap();
        registry.register(descriptor);
    }

    // Preprocess synchronously — we need it to prove
    info!("[clawproof] Preprocessing uploaded model {} for immediate proof", model_id);
    let preprocess_onnx_path = onnx_path.clone();
    let preprocess_trace = trace_length;
    let preprocessing = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let model_fn = || model(&preprocess_onnx_path);
            Snark::prover_preprocess(model_fn, preprocess_trace)
        }))
    })
    .await
    .map_err(|e| {
        error!("[clawproof] Preprocessing task failed for {}: {:?}", model_id, e);
        let _ = std::fs::remove_dir_all(&model_dir);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Model preprocessing failed".to_string(),
                hint: Some("The model may use unsupported ONNX operations".to_string()),
            }),
        )
    })?
    .map_err(|_| {
        error!("[clawproof] Preprocessing panicked for {}", model_id);
        let _ = std::fs::remove_dir_all(&model_dir);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Model preprocessing crashed — likely uses unsupported operations".to_string(),
                hint: Some("Use a simpler ONNX model with supported ops (Gemm, Relu, Add, etc.)".to_string()),
            }),
        )
    })?;

    let verifier_preprocessing = (&preprocessing).into();
    state.preprocessing.insert(
        model_id.clone(),
        PreprocessingCache {
            prover: preprocessing,
            verifier: verifier_preprocessing,
        },
    );
    info!("[clawproof] Model {} preprocessed, running proof", model_id);

    // Now prove
    let prove_input = ProveInput {
        text: None,
        fields: None,
        raw: Some(input_raw),
    };

    let result = run_single_prove(&state, model_id.clone(), prove_input, webhook_url).await?;

    Ok(Json(ProveModelResponse {
        receipt_id: result.receipt_id,
        receipt_url: result.receipt_url,
        model_id: result.model_id,
        output: result.output,
        status: result.status,
    }))
}
