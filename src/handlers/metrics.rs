use axum::extract::State;
use axum::Json;

use crate::receipt::ReceiptStats;
use crate::state::AppState;

pub async fn metrics(State(state): State<AppState>) -> Json<ReceiptStats> {
    Json(state.receipts.get_stats())
}
