use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// Returns the JSON array of MCP tool definitions with name, description, and inputSchema.
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_models",
            "description": "List all available ML models on the ClawProof server. Returns model IDs, names, descriptions, input types, labels, and configuration details.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "prove",
            "description": "Run zkML proof generation for a model. Submits an inference request, then polls until the proof is complete (status becomes 'verified' or 'failed'). Returns the full receipt including cryptographic hashes, proof metadata, and prediction output.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "model_id": {
                        "type": "string",
                        "description": "The model ID to run inference and proof on (e.g. 'authorization')"
                    },
                    "input_json": {
                        "type": "string",
                        "description": "JSON string representing the input object. For structured_fields models: {\"fields\": {\"field\": value}}. For text models: {\"text\": \"...\"}. For raw models: {\"raw\": [1, 2, 3, ...]}"
                    }
                },
                "required": ["model_id", "input_json"]
            }
        },
        {
            "name": "verify",
            "description": "Verify a previously generated zkML proof receipt. Returns whether the proof is valid, along with the receipt ID and current status.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "receipt_id": {
                        "type": "string",
                        "description": "The UUID of the receipt to verify"
                    }
                },
                "required": ["receipt_id"]
            }
        },
        {
            "name": "get_receipt",
            "description": "Retrieve a proof receipt by ID. Returns the full receipt JSON including model info, cryptographic hashes (model_hash, input_hash, output_hash, proof_hash), inference output, timing data, and status.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "receipt_id": {
                        "type": "string",
                        "description": "The UUID of the receipt to retrieve"
                    }
                },
                "required": ["receipt_id"]
            }
        },
        {
            "name": "upload_model",
            "description": "Upload a custom ONNX model to ClawProof. The model will be registered and preprocessed for proof generation. Maximum file size is 5MB. The model must accept raw integer input vectors.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the ONNX model file on the local filesystem"
                    },
                    "name": {
                        "type": "string",
                        "description": "Human-readable name for the model"
                    },
                    "labels": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Output class labels (e.g. [\"cat\", \"dog\"])"
                    },
                    "input_dim": {
                        "type": "integer",
                        "description": "Number of input dimensions (length of the input vector)"
                    },
                    "trace_length": {
                        "type": "integer",
                        "description": "Jolt trace length as a power of 2 (default: 16384 = 2^14). Larger models need larger traces."
                    }
                },
                "required": ["file_path", "name", "labels", "input_dim"]
            }
        }
    ])
}

/// Dispatch a tool call by name to the appropriate handler.
pub async fn call_tool(
    client: &reqwest::Client,
    base_url: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    match tool_name {
        "list_models" => handle_list_models(client, base_url).await,
        "prove" => handle_prove(client, base_url, &arguments).await,
        "verify" => handle_verify(client, base_url, &arguments).await,
        "get_receipt" => handle_get_receipt(client, base_url, &arguments).await,
        "upload_model" => handle_upload_model(client, base_url, &arguments).await,
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

async fn handle_list_models(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<Value, String> {
    let url = format!("{}/models", base_url);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request to {} failed: {}", url, e))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response JSON: {}", e))?;

    if !status.is_success() {
        return Err(format!("GET /models returned {}: {}", status, body));
    }

    Ok(body)
}

async fn handle_prove(
    client: &reqwest::Client,
    base_url: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let model_id = arguments
        .get("model_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: model_id".to_string())?;

    let input_json_str = arguments
        .get("input_json")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: input_json".to_string())?;

    let input_value: Value = serde_json::from_str(input_json_str)
        .map_err(|e| format!("Invalid input_json: {}", e))?;

    // Build the prove request body
    let prove_body = json!({
        "model_id": model_id,
        "input": input_value
    });

    // POST /prove
    let url = format!("{}/prove", base_url);
    let resp = client
        .post(&url)
        .json(&prove_body)
        .send()
        .await
        .map_err(|e| format!("POST /prove failed: {}", e))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse /prove response: {}", e))?;

    if !status.is_success() {
        return Err(format!("POST /prove returned {}: {}", status, body));
    }

    let receipt_id = body
        .get("receipt_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "No receipt_id in /prove response".to_string())?
        .to_string();

    // Poll GET /receipt/{id} until status is no longer "proving"
    let receipt_url = format!("{}/receipt/{}", base_url, receipt_id);
    let max_polls = 120; // up to ~10 minutes at 5s intervals
    let poll_interval = Duration::from_secs(5);

    for attempt in 0..max_polls {
        if attempt > 0 {
            tokio::time::sleep(poll_interval).await;
        }

        let poll_resp = client
            .get(&receipt_url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("GET /receipt/{} failed: {}", receipt_id, e))?;

        let poll_body: Value = poll_resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse receipt JSON: {}", e))?;

        let status_str = poll_body
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match status_str {
            "verified" | "failed" => return Ok(poll_body),
            "proving" => continue,
            other => {
                return Err(format!(
                    "Unexpected receipt status '{}': {}",
                    other, poll_body
                ))
            }
        }
    }

    Err(format!(
        "Proof generation timed out after {} polls for receipt {}",
        max_polls, receipt_id
    ))
}

async fn handle_verify(
    client: &reqwest::Client,
    base_url: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let receipt_id = arguments
        .get("receipt_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: receipt_id".to_string())?;

    let url = format!("{}/verify", base_url);
    let body = json!({ "receipt_id": receipt_id });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("POST /verify failed: {}", e))?;

    let status = resp.status();
    let resp_body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse /verify response: {}", e))?;

    if !status.is_success() {
        return Err(format!("POST /verify returned {}: {}", status, resp_body));
    }

    Ok(resp_body)
}

async fn handle_get_receipt(
    client: &reqwest::Client,
    base_url: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let receipt_id = arguments
        .get("receipt_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: receipt_id".to_string())?;

    let url = format!("{}/receipt/{}", base_url, receipt_id);
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("GET /receipt/{} failed: {}", receipt_id, e))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse receipt JSON: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "GET /receipt/{} returned {}: {}",
            receipt_id, status, body
        ));
    }

    Ok(body)
}

async fn handle_upload_model(
    client: &reqwest::Client,
    base_url: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let file_path = arguments
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: file_path".to_string())?;

    let name = arguments
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: name".to_string())?;

    let labels = arguments
        .get("labels")
        .ok_or_else(|| "Missing required argument: labels".to_string())?;

    let input_dim = arguments
        .get("input_dim")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "Missing required argument: input_dim (must be a positive integer)".to_string())?;

    let trace_length = arguments
        .get("trace_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384); // default 2^14

    // Read the ONNX file from disk
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let file_bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "network.onnx".to_string());

    // Serialize labels to JSON string
    let labels_json = serde_json::to_string(labels)
        .map_err(|e| format!("Failed to serialize labels: {}", e))?;

    // Build multipart form
    let onnx_part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("application/octet-stream")
        .map_err(|e| format!("Failed to create multipart part: {}", e))?;

    let form = reqwest::multipart::Form::new()
        .part("onnx_file", onnx_part)
        .text("name", name.to_string())
        .text("input_dim", input_dim.to_string())
        .text("labels", labels_json)
        .text("trace_length", trace_length.to_string());

    let url = format!("{}/models/upload", base_url);
    let resp = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("POST /models/upload failed: {}", e))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse /models/upload response: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "POST /models/upload returned {}: {}",
            status, body
        ));
    }

    Ok(body)
}
