use axum::http::header;
use axum::response::{IntoResponse, Response};

pub async fn openapi_spec() -> Response {
    let spec = include_str!("../openapi_spec.json");
    (
        [(header::CONTENT_TYPE, "application/json")],
        spec,
    )
        .into_response()
}
