use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::receipt::ReceiptSummary;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct RecentParams {
    pub limit: Option<u64>,
}

pub async fn recent(
    State(state): State<AppState>,
    Query(params): Query<RecentParams>,
) -> Json<Vec<ReceiptSummary>> {
    let limit = params.limit.unwrap_or(10).min(50);
    Json(state.receipts.list_recent(limit))
}
