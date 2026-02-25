use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tracing::error;

use super::prove::ErrorResponse;
use crate::state::AppState;

pub async fn convert(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Response {
    let converter_url = match &state.config.converter_url {
        Some(url) => url.clone(),
        None => {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(ErrorResponse {
                    error: "Model converter not configured".to_string(),
                    hint: Some("Set CONVERTER_URL environment variable to enable conversion".to_string()),
                }),
            )
                .into_response();
        }
    };

    // Forward the multipart form to the converter service
    let client = reqwest::Client::new();
    let url = format!("{}/convert", converter_url);

    // Read all fields and forward them
    let mut form = reqwest::multipart::Form::new();
    let mut mp = multipart;
    while let Ok(Some(field)) = mp.next_field().await {
        let name: String = field.name().unwrap_or("file").to_string();
        let bytes: axum::body::Bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("Failed to read upload: {}", e),
                        hint: None,
                    }),
                )
                    .into_response();
            }
        };
        let part = reqwest::multipart::Part::bytes(bytes.to_vec()).file_name(name.clone());
        form = form.part(name, part);
    }

    match client.post(&url).multipart(form).send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let body = resp.bytes().await.unwrap_or_default();
            (
                status,
                [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
                body,
            )
                .into_response()
        }
        Err(e) => {
            error!("[clawproof] Converter proxy failed: {:?}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: "Converter service unavailable".to_string(),
                    hint: Some("The model conversion sidecar is not responding".to_string()),
                }),
            )
                .into_response()
        }
    }
}
