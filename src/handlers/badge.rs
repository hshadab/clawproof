use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::receipt::ReceiptStatus;
use crate::state::AppState;

pub async fn badge(
    State(state): State<AppState>,
    Path(receipt_id): Path<String>,
) -> Response {
    let receipt = match state.receipts.get(&receipt_id) {
        Some(r) => r,
        None => {
            return (StatusCode::NOT_FOUND, "Receipt not found").into_response();
        }
    };

    let (status_text, color, bg_color, cache_control) = match receipt.status {
        ReceiptStatus::Proving => ("proving", "#856404", "#fff3cd", "no-cache"),
        ReceiptStatus::Verified => ("verified", "#155724", "#d4edda", "public, max-age=3600"),
        ReceiptStatus::Failed => ("failed", "#721c24", "#f8d7da", "public, max-age=3600"),
    };

    let label = "ClawProof";
    let label_width = label.len() as u32 * 7 + 10;
    let value_width = status_text.len() as u32 * 7 + 10;
    let total_width = label_width + value_width;

    let label_x = label_width / 2;
    let value_x = label_width + value_width / 2;
    let white = "#fff";
    let gray = "#555";
    let grad_stop = "#bbb";
    let shadow = "#010101";

    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{total_width}" height="20" role="img" aria-label="{label}: {status_text}">
  <title>{label}: {status_text}</title>
  <linearGradient id="s" x2="0" y2="100%">
    <stop offset="0" stop-color="{grad_stop}" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <clipPath id="r"><rect width="{total_width}" height="20" rx="3" fill="{white}"/></clipPath>
  <g clip-path="url(#r)">
    <rect width="{label_width}" height="20" fill="{gray}"/>
    <rect x="{label_width}" width="{value_width}" height="20" fill="{bg_color}"/>
    <rect width="{total_width}" height="20" fill="url(#s)"/>
  </g>
  <g fill="{white}" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" text-rendering="geometricPrecision" font-size="110">
    <text aria-hidden="true" x="{label_x}0" y="150" fill="{shadow}" fill-opacity=".3" transform="scale(.1)">{label}</text>
    <text x="{label_x}0" y="140" transform="scale(.1)">{label}</text>
    <text aria-hidden="true" x="{value_x}0" y="150" fill="{shadow}" fill-opacity=".3" transform="scale(.1)">{status_text}</text>
    <text x="{value_x}0" y="140" transform="scale(.1)" fill="{color}">{status_text}</text>
  </g>
</svg>"#
    );

    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, cache_control),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        svg,
    )
        .into_response()
}
