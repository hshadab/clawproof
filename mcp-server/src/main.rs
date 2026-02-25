mod tools;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// JSON-RPC 2.0 request envelope.
#[derive(Deserialize, Debug)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// JSON-RPC 2.0 response envelope.
#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

const SERVER_NAME: &str = "clawproof-mcp";
const SERVER_VERSION: &str = "0.1.0";
const PROTOCOL_VERSION: &str = "2024-11-05";

fn success_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn error_response(id: Value, code: i64, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
    }
}

fn write_response(response: &JsonRpcResponse) {
    let output = serde_json::to_string(response).expect("Failed to serialize response");
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(output.as_bytes());
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// Handle the "initialize" method: return server info and capabilities.
fn handle_initialize(id: Value) -> JsonRpcResponse {
    success_response(
        id,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        }),
    )
}

/// Handle the "tools/list" method: return all tool definitions.
fn handle_tools_list(id: Value) -> JsonRpcResponse {
    success_response(
        id,
        json!({
            "tools": tools::tool_definitions()
        }),
    )
}

/// Handle the "tools/call" method: dispatch to the appropriate tool handler.
async fn handle_tools_call(
    id: Value,
    params: &Value,
    client: &reqwest::Client,
    base_url: &str,
) -> JsonRpcResponse {
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return error_response(
                id,
                -32602,
                "Missing 'name' in tools/call params".to_string(),
            );
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match tools::call_tool(client, base_url, tool_name, arguments).await {
        Ok(result) => {
            // MCP tools/call returns content array
            let text = match serde_json::to_string_pretty(&result) {
                Ok(s) => s,
                Err(_) => result.to_string(),
            };
            success_response(
                id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }),
            )
        }
        Err(err_msg) => success_response(
            id,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": err_msg
                    }
                ],
                "isError": true
            }),
        ),
    }
}

/// Process a single JSON-RPC request line.
async fn process_request(
    line: &str,
    client: &reqwest::Client,
    base_url: &str,
) {
    let request: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let resp = error_response(
                Value::Null,
                -32700,
                format!("Parse error: {}", e),
            );
            write_response(&resp);
            return;
        }
    };

    let id = request.id.clone().unwrap_or(Value::Null);

    // Notifications (no id) for methods like "notifications/initialized" --
    // the spec says we must not reply to notifications.
    if request.id.is_none() {
        // Silently accept notifications
        return;
    }

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(id),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(id, &request.params, client, base_url).await,
        "ping" => success_response(id, json!({})),
        _ => error_response(
            id,
            -32601,
            format!("Method not found: {}", request.method),
        ),
    };

    write_response(&response);
}

#[tokio::main]
async fn main() {
    let base_url = std::env::var("CLAWPROOF_URL")
        .unwrap_or_else(|_| "https://clawproof.onrender.com".to_string());

    // Trim trailing slash for consistency
    let base_url = base_url.trim_end_matches('/').to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .expect("Failed to build HTTP client");

    // Read JSON-RPC messages from stdin, one per line
    let stdin = io::stdin();
    let reader = stdin.lock();

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break, // stdin closed
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        process_request(trimmed, &client, &base_url).await;
    }
}
