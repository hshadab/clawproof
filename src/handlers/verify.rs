use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::prove::ErrorResponse;
use crate::receipt::ReceiptStatus;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub receipt_id: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub valid: bool,
    pub receipt_id: String,
    pub status: String,
}

pub async fn verify(
    State(state): State<AppState>,
    Json(request): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let receipt = state.receipts.get(&request.receipt_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Receipt not found".into(),
                hint: Some("Check the receipt_id and try GET /receipt/{id}".to_string()),
            }),
        )
    })?;

    match receipt.status {
        ReceiptStatus::Verified => Ok(Json(VerifyResponse {
            valid: true,
            receipt_id: receipt.id,
            status: "verified".to_string(),
        })),
        ReceiptStatus::Proving => Ok(Json(VerifyResponse {
            valid: false,
            receipt_id: receipt.id,
            status: "proving".to_string(),
        })),
        ReceiptStatus::Failed => Ok(Json(VerifyResponse {
            valid: false,
            receipt_id: receipt.id,
            status: "failed".to_string(),
        })),
    }
}
