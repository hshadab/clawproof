use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::prove::{ErrorResponse, ProveInput, ProveResponse};
use crate::handlers::prove::run_single_prove;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct BatchRequest {
    pub requests: Vec<BatchItem>,
}

#[derive(Deserialize)]
pub struct BatchItem {
    pub model_id: String,
    #[serde(default)]
    pub input: ProveInput,
    #[serde(default)]
    pub webhook_url: Option<String>,
}

#[derive(Serialize)]
pub struct BatchResponse {
    pub receipts: Vec<ProveResponse>,
}

pub async fn batch_prove(
    State(state): State<AppState>,
    Json(request): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    if request.requests.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "At least one request is required".to_string(),
                hint: Some("Provide {\"requests\": [{\"model_id\": \"...\", \"input\": {...}}]}".to_string()),
            }),
        ));
    }

    if request.requests.len() > 5 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Maximum 5 requests per batch".to_string(),
                hint: None,
            }),
        ));
    }

    let mut receipts = Vec::new();
    for item in request.requests {
        let result = run_single_prove(&state, item.model_id, item.input, item.webhook_url).await?;
        receipts.push(result);
    }

    Ok(Json(BatchResponse { receipts }))
}
