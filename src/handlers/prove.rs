use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::Utc;
use onnx_tracer::{model, tensor::Tensor};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info};

use crate::crypto;
use crate::geo;
use crate::input::{build_onehot_vector, build_tfidf_vector, build_token_index_vector};
use crate::models::InputType;
use crate::prover;
use crate::receipt::{InferenceOutput, Receipt, ReceiptStatus};
use crate::state::{AppState, VocabData};

#[derive(Deserialize)]
pub struct ProveRequest {
    pub model_id: String,
    #[serde(default)]
    pub input: ProveInput,
    #[serde(default)]
    pub webhook_url: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ProveInput {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub fields: Option<HashMap<String, usize>>,
    #[serde(default)]
    pub raw: Option<Vec<i32>>,
}

#[derive(Serialize, Clone)]
pub struct ProveResponse {
    pub receipt_id: String,
    pub receipt_url: String,
    pub model_id: String,
    pub output: InferenceOutput,
    pub status: String,
    pub proof_string: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

pub async fn prove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ProveRequest>,
) -> Result<Json<ProveResponse>, (StatusCode, Json<ErrorResponse>)> {
    let client_ip = extract_client_ip(&headers);
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    run_single_prove(&state, request.model_id, request.input, request.webhook_url, client_ip, user_agent).await
        .map(Json)
}

/// Extract client IP from X-Forwarded-For header (first entry) or fall back to other headers.
pub fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        })
}

pub async fn run_single_prove(
    state: &AppState,
    model_id: String,
    input: ProveInput,
    webhook_url: Option<String>,
    client_ip: Option<String>,
    user_agent: Option<String>,
) -> Result<ProveResponse, (StatusCode, Json<ErrorResponse>)> {
    // Validate webhook URL if provided
    if let Some(ref url) = webhook_url {
        if !url.starts_with("https://") {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "webhook_url must use HTTPS".to_string(),
                    hint: Some("Provide a URL starting with https://".to_string()),
                }),
            ));
        }
    }

    let model_desc = {
        let registry = state.registry.read().expect("model registry lock poisoned");
        registry.get(&model_id).cloned().ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Model not found: {}", model_id),
                    hint: Some("Check GET /models for available model IDs".to_string()),
                }),
            )
        })?
    };

    if !state.preprocessing.contains_key(&model_id) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: format!(
                    "Model '{}' is still loading. Try again shortly.",
                    model_id
                ),
                hint: Some("Check GET /health to see model loading status".to_string()),
            }),
        ));
    }

    // Build input vector based on model type
    let input_vector: Vec<i32> = match &model_desc.input_type {
        InputType::Text => {
            let text = input.text.as_deref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Text input required for this model".to_string(),
                        hint: Some("Provide {\"input\": {\"text\": \"...\"}}".to_string()),
                    }),
                )
            })?;

            if text.len() > 10_000 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Text input exceeds maximum length of 10,000 characters".into(),
                        hint: None,
                    }),
                ));
            }

            let vocab = state.vocabs.get(&model_id).ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Vocabulary not loaded".to_string(),
                        hint: None,
                    }),
                )
            })?;

            match vocab {
                VocabData::TfIdf(v) => build_tfidf_vector(text, v, model_desc.input_dim),
                VocabData::TokenIndex(v) => build_token_index_vector(text, v, model_desc.input_dim),
                _ => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Wrong vocabulary type for model".to_string(),
                            hint: None,
                        }),
                    ))
                }
            }
        }
        InputType::StructuredFields => {
            let fields = input.fields.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Field inputs required for this model".to_string(),
                        hint: Some("Provide {\"input\": {\"fields\": {\"field_name\": value}}}".to_string()),
                    }),
                )
            })?;

            if let Some(schemas) = &model_desc.fields {
                for schema in schemas {
                    if let Some(&value) = fields.get(&schema.name) {
                        if value > schema.max {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    error: format!(
                                        "Field '{}' value {} exceeds max {}",
                                        schema.name, value, schema.max
                                    ),
                                    hint: None,
                                }),
                            ));
                        }
                    }
                }
            }

            let vocab = state.vocabs.get(&model_id).ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Vocabulary not loaded".to_string(),
                        hint: None,
                    }),
                )
            })?;

            let field_names: Vec<&str> = model_desc
                .fields
                .as_ref()
                .map(|fs| fs.iter().map(|f| f.name.as_str()).collect())
                .unwrap_or_default();

            match vocab {
                VocabData::OneHot(v) => {
                    build_onehot_vector(fields, &field_names, v, model_desc.input_dim)
                }
                _ => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Wrong vocabulary type for model".to_string(),
                            hint: None,
                        }),
                    ))
                }
            }
        }
        InputType::Raw => {
            let raw = input.raw.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Raw input vector required for this model".to_string(),
                        hint: Some(format!("Provide {{\"input\": {{\"raw\": [...]}}}} with {} elements", model_desc.input_dim)),
                    }),
                )
            })?;

            if raw.len() != model_desc.input_dim {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!(
                            "Raw input length {} does not match expected {}",
                            raw.len(),
                            model_desc.input_dim
                        ),
                        hint: None,
                    }),
                ));
            }

            raw.clone()
        }
    };

    // Create tensor
    let input_tensor =
        Tensor::new(Some(&input_vector), &model_desc.input_shape).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to create input tensor: {:?}", e),
                    hint: None,
                }),
            )
        })?;

    // Run inference (forward pass only)
    let model_path = state.config.resolve_model_path(&model_id);

    // Run inference in a blocking thread with panic protection to avoid
    // taking down the server if the ONNX tracer panics.
    let inference_path = model_path.clone();
    let inference_tensor = input_tensor.clone();
    let raw_output: Vec<i32> = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let model_instance = model(&inference_path);
            let result = model_instance
                .forward(&[inference_tensor])
                .map_err(|e| format!("{}", e))?;
            Ok::<_, String>(result.outputs[0].data().to_vec())
        }))
    })
    .await
    .map_err(|e| {
        error!("[clawproof] Inference task failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Inference task failed".to_string(),
                hint: None,
            }),
        )
    })?
    .map_err(|_| {
        error!("[clawproof] Inference panicked for model {}", model_id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Inference crashed — model may use unsupported operations".to_string(),
                hint: Some("Try a simpler ONNX model or check supported ops".to_string()),
            }),
        )
    })?
    .map_err(|e| {
        error!("[clawproof] Inference error for model {}: {:?}", model_id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Inference failed: {}", e),
                hint: None,
            }),
        )
    })?;

    // Determine prediction
    let (pred_idx, _max_val) = raw_output
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.cmp(b.1))
        .unwrap_or((0, &0));

    let label = model_desc
        .labels
        .get(pred_idx)
        .cloned()
        .unwrap_or_else(|| format!("class_{}", pred_idx));

    // Softmax confidence: exp(x_i) / sum(exp(x_j)) with numerical stability
    let confidence = {
        let vals: Vec<f64> = raw_output.iter().map(|&x| x as f64).collect();
        let max_val = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exp_sum: f64 = vals.iter().map(|&v| (v - max_val).exp()).sum();
        if exp_sum > 0.0 {
            (vals[pred_idx] - max_val).exp() / exp_sum
        } else {
            0.0
        }
    };

    // Compute hashes
    let input_hash = crypto::hash_tensor(&input_vector);
    let output_hash = crypto::hash_tensor(&raw_output);
    let model_hash = if let Some(ref cached) = model_desc.model_hash {
        cached.clone()
    } else {
        crypto::compute_model_commitment(&model_path).map_err(|e| {
            error!("[clawproof] Failed to compute model commitment: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to compute model hash".to_string(),
                    hint: None,
                }),
            )
        })?
    };

    // Create receipt
    let receipt_id = uuid::Uuid::new_v4().to_string();
    let output = InferenceOutput {
        raw_output,
        predicted_class: pred_idx,
        label: label.clone(),
        confidence,
    };

    let receipt = Receipt {
        id: receipt_id.clone(),
        model_id: model_id.clone(),
        model_name: model_desc.name.clone(),
        status: ReceiptStatus::Proving,
        created_at: Utc::now(),
        completed_at: None,
        model_hash,
        input_hash,
        output_hash,
        output: output.clone(),
        proof_hash: None,
        proof_size: None,
        prove_time_ms: None,
        verify_time_ms: None,
        error: None,
        client_ip: client_ip.clone(),
        user_agent,
        geo_city: None,
        geo_country: None,
    };

    state.receipts.insert(receipt);

    // Spawn async geo lookup (fire-and-forget)
    if let Some(ref ip) = client_ip {
        let http_client = state.http_client.clone();
        let receipts = state.receipts.clone();
        let ip = ip.clone();
        let rid = receipt_id.clone();
        tokio::spawn(async move {
            let (city, country) = geo::lookup(&http_client, &ip).await;
            if city.is_some() || country.is_some() {
                receipts.update_geo(&rid, city, country);
            }
        });
    }

    info!(
        "[clawproof] Receipt {} created, spawning proof for model {}",
        receipt_id, model_id
    );

    prover::prove_and_verify(
        receipt_id.clone(),
        state.receipts.clone(),
        state.preprocessing.clone(),
        model_id.clone(),
        state.config.clone(),
        input_tensor,
        webhook_url,
    );

    let receipt_url = format!("{}/receipt/{}", state.config.base_url, receipt_id);

    let proof_string = format!("clawproof:{}:{}:proving", receipt_id, output.label);

    Ok(ProveResponse {
        receipt_id,
        receipt_url,
        model_id,
        output,
        status: "proving".to_string(),
        proof_string,
    })
}
