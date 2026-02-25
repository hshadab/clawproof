use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use tracing::{error, info};

use super::prove::ErrorResponse;
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
pub struct UploadResponse {
    pub model_id: String,
    pub name: String,
    pub status: String,
}

pub async fn upload_model(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut onnx_bytes: Option<Vec<u8>> = None;
    let mut name: Option<String> = None;
    let mut description = String::new();
    let mut input_dim: usize = 0;
    let mut labels: Vec<String> = Vec::new();
    let mut trace_length: usize = 1 << 14;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name: String = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "onnx_file" => {
                let bytes: axum::body::Bytes = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Failed to read ONNX file: {}", e),
                            hint: None,
                        }),
                    )
                })?;
                if bytes.len() > 5 * 1024 * 1024 {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(ErrorResponse {
                            error: "ONNX file exceeds 5MB limit".to_string(),
                            hint: None,
                        }),
                    ));
                }
                onnx_bytes = Some(bytes.to_vec());
            }
            "name" => {
                name = Some(field.text().await.unwrap_or_default());
            }
            "description" => {
                description = field.text().await.unwrap_or_default();
            }
            "input_dim" => {
                let text: String = field.text().await.unwrap_or_default();
                input_dim = text.parse().unwrap_or(0);
            }
            "labels" => {
                let text: String = field.text().await.unwrap_or_default();
                labels = serde_json::from_str(&text).unwrap_or_default();
            }
            "trace_length" => {
                let text: String = field.text().await.unwrap_or_default();
                trace_length = text.parse().unwrap_or(1 << 14);
            }
            _ => {}
        }
    }

    let onnx_bytes = onnx_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing onnx_file field".to_string(),
                hint: Some("Upload ONNX model as multipart form field 'onnx_file'".to_string()),
            }),
        )
    })?;

    let name = name.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing name field".to_string(),
                hint: None,
            }),
        )
    })?;

    if input_dim == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "input_dim must be > 0".to_string(),
                hint: None,
            }),
        ));
    }

    if labels.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "labels must be a non-empty JSON array".to_string(),
                hint: Some("Provide labels as JSON array string, e.g. '[\"class_a\",\"class_b\"]'".to_string()),
            }),
        ));
    }

    // Generate model ID
    let model_id = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let model_id = format!(
        "{}_{}",
        model_id,
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    // Save ONNX file
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

    // Validate by loading
    let onnx_path_clone = onnx_path.clone();
    let validation_result = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(|| {
            let _ = model(&onnx_path_clone);
        })
    })
    .await;

    match validation_result {
        Ok(Ok(())) => {}
        _ => {
            let _ = std::fs::remove_dir_all(&model_dir);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid ONNX model — failed to load".to_string(),
                    hint: Some("Ensure the file is a valid ONNX model".to_string()),
                }),
            ));
        }
    }

    // Generate model.toml
    let toml_content = format!(
        r#"id = "{model_id}"
name = "{name}"
description = "{description}"
input_type = "raw"
input_dim = {input_dim}
input_shape = [1, {input_dim}]
labels = [{labels_str}]
trace_length = {trace_length}
"#,
        model_id = model_id,
        name = name,
        description = description.replace('"', "\\\""),
        input_dim = input_dim,
        labels_str = labels
            .iter()
            .map(|l| format!("\"{}\"", l))
            .collect::<Vec<_>>()
            .join(", "),
        trace_length = trace_length,
    );
    let _ = std::fs::write(model_dir.join("model.toml"), &toml_content);

    // Register in model registry
    let descriptor = ModelDescriptor {
        id: model_id.clone(),
        name: name.clone(),
        description,
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

    // Spawn background preprocessing
    let bg_state = state.clone();
    let bg_model_id = model_id.clone();
    let bg_model_path = onnx_path;
    tokio::spawn(async move {
        info!("[clawproof] Starting preprocessing for uploaded model {}", bg_model_id);
        let result = tokio::task::spawn_blocking(move || {
            let model_fn = || model(&bg_model_path);
            Snark::prover_preprocess(model_fn, trace_length)
        })
        .await;

        match result {
            Ok(preprocessing) => {
                let verifier_preprocessing = (&preprocessing).into();
                bg_state.preprocessing.insert(
                    bg_model_id.clone(),
                    PreprocessingCache {
                        prover: preprocessing,
                        verifier: verifier_preprocessing,
                    },
                );
                info!("[clawproof] Uploaded model {} preprocessed successfully", bg_model_id);
            }
            Err(e) => {
                error!("[clawproof] Failed to preprocess uploaded model {}: {:?}", bg_model_id, e);
            }
        }
    });

    Ok(Json(UploadResponse {
        model_id,
        name,
        status: "preprocessing".to_string(),
    }))
}
