use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use serde::Serialize;
use tracing::info;

use crate::state::AppState;

#[derive(Serialize)]
pub struct UpdateResponse {
    pub status: String,
}

/// PUT /admin/static/playground
///
/// Replace the live playground.html on the persistent disk.
/// Requires `Authorization: Bearer <ADMIN_SECRET>` header.
/// Body is the raw HTML string.
pub async fn update_playground(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<UpdateResponse>, StatusCode> {
    // Check admin secret
    let secret = std::env::var("ADMIN_SECRET").unwrap_or_default();
    if secret.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if auth != format!("Bearer {}", secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Write to STATIC_DIR
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".to_string());
    let path = std::path::Path::new(&static_dir).join("playground.html");

    std::fs::create_dir_all(&static_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    std::fs::write(&path, &body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("[clawproof] Playground HTML updated via admin endpoint ({} bytes)", body.len());

    Ok(Json(UpdateResponse {
        status: "updated".to_string(),
    }))
}
