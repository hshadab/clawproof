use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::state::AppState;
use crate::templates::receipt_page;

#[derive(Deserialize, Default)]
pub struct ReceiptQuery {
    #[serde(default)]
    pub format: Option<String>,
}

pub async fn get_receipt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ReceiptQuery>,
    headers: HeaderMap,
) -> Response {
    let receipt = match state.receipts.get(&id) {
        Some(r) => r,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Receipt not found", "hint": "Check the receipt ID"})),
            )
                .into_response();
        }
    };

    // Check for ?format=jsonld
    if query.format.as_deref() == Some("jsonld") {
        let jsonld = serde_json::json!({
            "@context": "https://schema.org",
            "@type": "DigitalDocument",
            "identifier": receipt.id,
            "name": format!("zkML Proof Receipt — {}", receipt.model_name),
            "description": format!("Cryptographic proof of ML inference for model '{}'", receipt.model_id),
            "dateCreated": receipt.created_at.to_rfc3339(),
            "dateModified": receipt.completed_at.map(|t| t.to_rfc3339()),
            "creator": {
                "@type": "SoftwareApplication",
                "name": "ClawProof",
                "url": &state.config.base_url,
            },
            "about": {
                "@type": "SoftwareApplication",
                "name": &receipt.model_name,
                "identifier": &receipt.model_id,
            },
            "status": receipt.status.as_str(),
            "model_hash": &receipt.model_hash,
            "input_hash": &receipt.input_hash,
            "output_hash": &receipt.output_hash,
            "proof_hash": &receipt.proof_hash,
            "proof_size": receipt.proof_size,
            "prove_time_ms": receipt.prove_time_ms.map(|t| t as u64),
            "verify_time_ms": receipt.verify_time_ms.map(|t| t as u64),
            "prediction": {
                "label": &receipt.output.label,
                "confidence": receipt.output.confidence,
                "predicted_class": receipt.output.predicted_class,
            },
        });

        return (
            [(header::CONTENT_TYPE, "application/ld+json")],
            serde_json::to_string_pretty(&jsonld).unwrap_or_default(),
        )
            .into_response();
    }

    // Content negotiation: JSON if Accept: application/json, HTML otherwise
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/html");

    if accept.contains("application/json") {
        let mut json = serde_json::to_value(&receipt).unwrap_or_default();
        let status_str = receipt.status.as_str();
        let proof_string = format!("clawproof:{}:{}:{}", receipt.id, receipt.output.label, status_str);
        if let Some(obj) = json.as_object_mut() {
            obj.insert("proof_string".to_string(), serde_json::Value::String(proof_string));
        }
        Json(json).into_response()
    } else {
        let html = receipt_page::render(&receipt, &state.config.base_url);
        Html(html).into_response()
    }
}
