use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub proof_system: String,
    pub models_loaded: usize,
    pub models_total: usize,
    pub ready: bool,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let loaded = state.preprocessing.len();
    let total = state.registry.read().unwrap().list().len();
    Json(HealthResponse {
        status: "ok".to_string(),
        version: "clawproof-v0.1.0".to_string(),
        proof_system: "JOLT-Atlas SNARK (Dory/BN254)".to_string(),
        models_loaded: loaded,
        models_total: total,
        ready: loaded == total,
    })
}
