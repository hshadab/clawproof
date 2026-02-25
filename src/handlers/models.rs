use axum::extract::State;
use axum::Json;

use crate::models::ModelDescriptor;
use crate::state::AppState;

pub async fn list_models(State(state): State<AppState>) -> Json<Vec<ModelDescriptor>> {
    let registry = state.registry.read().unwrap();
    let models: Vec<ModelDescriptor> = registry.list().into_iter().cloned().collect();
    Json(models)
}
