mod config;
mod crypto;
mod handlers;
mod input;
mod models;
mod prover;
mod receipt;
mod state;
mod templates;

use axum::response::Html;
use axum::routing::{get, post, put};
use axum::Router;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;
use axum::error_handling::HandleErrorLayer;
use axum::http::StatusCode;
use tower::ServiceBuilder;
use tower::limit::RateLimitLayer;
use tower::buffer::BufferLayer;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::info;

use onnx_tracer::model;

use crate::config::Config;
use crate::input::{load_onehot_vocab, load_tfidf_vocab, load_token_index_vocab};
use crate::models::{InputType, ModelRegistry};
use crate::receipt::ReceiptStore;
use crate::state::{AppState, PreprocessingCache, Snark, VocabData};

static RE_DUP: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(.)\1{2,}").unwrap());
static RE_WS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\s+").unwrap());

/// Solve Moltbook verification challenges (lobster-themed arithmetic).
/// Strips junk chars, extracts number words, determines operation, computes answer.
fn solve_moltbook_challenge(challenge: &str) -> Option<String> {
    // Strip non-alpha/space chars, normalize to lowercase
    let clean: String = challenge.chars()
        .map(|c| if c.is_alphabetic() || c.is_whitespace() { c.to_ascii_lowercase() } else { ' ' })
        .collect();
    // Collapse repeated letters (e.g., "looobster" -> "lobster", "thhree" -> "three")
    let clean = RE_DUP.replace_all(&clean, "$1$1");
    // Collapse whitespace
    let clean = RE_WS.replace_all(&clean, " ");

    let word_to_num: Vec<(&str, f64)> = vec![
        ("zero", 0.0), ("one", 1.0), ("two", 2.0), ("three", 3.0), ("four", 4.0),
        ("five", 5.0), ("six", 6.0), ("seven", 7.0), ("eight", 8.0), ("nine", 9.0),
        ("ten", 10.0), ("eleven", 11.0), ("twelve", 12.0), ("thirteen", 13.0),
        ("fourteen", 14.0), ("fifteen", 15.0), ("sixteen", 16.0), ("seventeen", 17.0),
        ("eighteen", 18.0), ("nineteen", 19.0), ("twenty", 20.0), ("thirty", 30.0),
        ("forty", 40.0), ("fifty", 50.0), ("sixty", 60.0), ("seventy", 70.0),
        ("eighty", 80.0), ("ninety", 90.0), ("hundred", 100.0),
    ];

    // Extract all number words in order and build compound numbers
    let words: Vec<&str> = clean.split_whitespace().collect();
    let mut numbers: Vec<f64> = Vec::new();
    let mut current: Option<f64> = None;

    for w in &words {
        if let Some(&(_, val)) = word_to_num.iter().find(|&&(name, _)| name == *w) {
            if val == 100.0 {
                // "hundred" multiplies the current accumulator
                current = Some(current.unwrap_or(1.0) * 100.0);
            } else if val >= 20.0 && val < 100.0 {
                // Tens place — start or extend a compound
                if let Some(c) = current {
                    if c < 20.0 {
                        // previous was a single digit that's part of a different number
                        numbers.push(c);
                        current = Some(val);
                    } else {
                        numbers.push(c);
                        current = Some(val);
                    }
                } else {
                    current = Some(val);
                }
            } else {
                // Units (0-19)
                if let Some(c) = current {
                    if c >= 20.0 && c % 10.0 == 0.0 && c < 100.0 {
                        // Compound: twenty + three = 23
                        current = Some(c + val);
                    } else {
                        numbers.push(c);
                        current = Some(val);
                    }
                } else {
                    current = Some(val);
                }
            }
        } else if current.is_some() {
            // Non-number word breaks the current compound
            if let Some(c) = current.take() {
                numbers.push(c);
            }
        }
    }
    if let Some(c) = current {
        numbers.push(c);
    }

    if numbers.len() < 2 {
        return None;
    }

    // Determine operation from cleaned text
    let is_subtract = clean.contains("slow") || clean.contains("lose")
        || clean.contains("less") || clean.contains("subtract")
        || clean.contains("minus") || clean.contains("decreas")
        || clean.contains("reduc") || clean.contains("drop")
        || clean.contains("fell") || clean.contains("lost");

    let is_multiply = clean.contains("times") || clean.contains("multipl")
        || clean.contains("product");

    let is_divide = clean.contains("divid") || clean.contains("split")
        || clean.contains("per each") || clean.contains("shared equal");

    let a = numbers[0];
    let b = numbers[1];

    let result = if is_subtract {
        a - b
    } else if is_multiply {
        a * b
    } else if is_divide && b != 0.0 {
        a / b
    } else {
        a + b // default: addition (total, combined, adds, etc.)
    };

    if result.fract() == 0.0 {
        Some(format!("{}", result as i64))
    } else {
        Some(format!("{:.2}", result))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    info!("[clawproof] Starting clawproof server");
    info!("[clawproof] Models directory: {:?}", config.models_dir);
    info!("[clawproof] Base URL: {}", config.base_url);
    info!("[clawproof] Database path: {:?}", config.database_path);

    // Initialize SQLite
    let receipts = ReceiptStore::new(&config.database_path)?;
    info!("[clawproof] SQLite receipt store initialized");

    let mut registry = ModelRegistry::new();

    // Scan built-in models directory
    registry.scan_directory(&config.models_dir);

    // Scan uploaded models directory for previously uploaded models
    registry.scan_directory(&config.uploaded_models_dir);

    // Load vocabularies
    info!("[clawproof] Loading vocabularies...");
    let mut vocabs = HashMap::new();
    for model_desc in registry.list() {
        // Check both built-in and uploaded model directories for vocab
        let vocab_path = {
            let builtin = config.models_dir.join(&model_desc.id).join("vocab.json");
            if builtin.exists() {
                builtin
            } else {
                config.uploaded_models_dir.join(&model_desc.id).join("vocab.json")
            }
        };
        match &model_desc.input_type {
            InputType::Text => {
                info!("[clawproof] Loading text vocab for {}", model_desc.id);
                // Try TF-IDF format first ({"word": {"index": N, "idf": F}})
                // Fall back to token-index format ({"word": N})
                match load_tfidf_vocab(&vocab_path) {
                    Ok(vocab) if !vocab.is_empty() => {
                        info!("[clawproof]   {} TF-IDF entries loaded", vocab.len());
                        vocabs.insert(model_desc.id.clone(), VocabData::TfIdf(vocab));
                    }
                    _ => match load_token_index_vocab(&vocab_path) {
                        Ok(vocab) if !vocab.is_empty() => {
                            info!("[clawproof]   {} token-index entries loaded", vocab.len());
                            vocabs.insert(model_desc.id.clone(), VocabData::TokenIndex(vocab));
                        }
                        _ => {
                            tracing::warn!("[clawproof] Failed to load vocab for {}", model_desc.id);
                        }
                    },
                }
            }
            InputType::StructuredFields => {
                info!("[clawproof] Loading one-hot vocab for {}", model_desc.id);
                match load_onehot_vocab(&vocab_path) {
                    Ok(vocab) => {
                        info!("[clawproof]   {} entries loaded", vocab.len());
                        vocabs.insert(model_desc.id.clone(), VocabData::OneHot(vocab));
                    }
                    Err(e) => {
                        tracing::warn!("[clawproof] Failed to load vocab for {}: {:?}", model_desc.id, e);
                    }
                }
            }
            InputType::Raw => {
                // Raw input type doesn't need vocabulary
                info!("[clawproof] Model {} uses raw input, no vocab needed", model_desc.id);
            }
        }
    }

    let registry = Arc::new(RwLock::new(registry));

    let state = AppState {
        config: config.clone(),
        receipts,
        registry: registry.clone(),
        vocabs: Arc::new(vocabs),
        preprocessing: Arc::new(dashmap::DashMap::new()),
    };

    // Spawn background preprocessing — server starts immediately so Render
    // health checks pass while models are being preprocessed.
    let bg_state = state.clone();
    let bg_config = config.clone();
    tokio::spawn(async move {
        info!("[clawproof] Starting background model preprocessing...");
        let model_list: Vec<_> = {
            let reg = bg_state.registry.read().expect("model registry lock poisoned");
            reg.list().into_iter().cloned().collect()
        };
        for model_desc in model_list {
            let model_id = model_desc.id.clone();
            let model_path = bg_config.resolve_model_path(&model_id);

            if !model_path.exists() {
                tracing::warn!("[clawproof] ONNX file not found for model {}, skipping", model_id);
                continue;
            }

            let trace_length = model_desc.trace_length;

            info!(
                "[clawproof] Preprocessing {} (trace_length: {})...",
                model_id, trace_length
            );

            let model_path_clone = model_path.clone();
            let preprocessing = match tokio::task::spawn_blocking(move || {
                let model_fn = || model(&model_path_clone);
                Snark::prover_preprocess(model_fn, trace_length)
            })
            .await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "[clawproof] Failed to preprocess {}: {:?}",
                        model_id,
                        e
                    );
                    continue;
                }
            };

            let verifier_preprocessing = (&preprocessing).into();
            info!("[clawproof] {} preprocessed successfully", model_id);

            bg_state.preprocessing.insert(
                model_id,
                PreprocessingCache {
                    prover: preprocessing,
                    verifier: verifier_preprocessing,
                },
            );
        }
        info!("[clawproof] All models preprocessed and ready");
    });

    // Periodic cache eviction (SQLite is persistent; DashMap is hot cache)
    let receipts_clone = state.receipts.clone();
    tokio::spawn(async move {
        let start = tokio::time::Instant::now() + Duration::from_secs(600);
        let mut interval = tokio::time::interval_at(start, Duration::from_secs(600));
        loop {
            interval.tick().await;
            receipts_clone.cleanup_cache(Duration::from_secs(3600));
        }
    });

    // Moltbook heartbeat — engagement cycle + combo posting every 30 min
    if let Some(ref key) = config.moltbook_api_key {
        let api_key = key.clone();
        let moltbook_receipts = state.receipts.clone();
        let moltbook_base_url = config.base_url.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let base = "https://www.moltbook.com/api/v1";
            let start = tokio::time::Instant::now() + Duration::from_secs(1800);
            let mut interval = tokio::time::interval_at(start, Duration::from_secs(1800));
            let mut cycle: u64 = 0;
            let mut consecutive_failures: u32 = 0;

            // Submolts to rotate through
            let submolts = ["tools", "ai", "programming", "crypto", "openclaw"];

            loop {
                interval.tick().await;

                // Exponential backoff after 3+ consecutive failures
                if consecutive_failures >= 3 {
                    let backoff_multiplier = 1u64 << (consecutive_failures - 3).min(3); // cap at 8x
                    let extra_sleep = Duration::from_secs(1800 * backoff_multiplier);
                    tracing::warn!(
                        "[moltbook] {} consecutive failures, backing off for {}s",
                        consecutive_failures,
                        extra_sleep.as_secs()
                    );
                    tokio::time::sleep(extra_sleep).await;
                }

                let auth = format!("Bearer {}", api_key);

                // --- Engagement: home, notifications, feed ---
                let _ = client.get(format!("{}/home", base))
                    .header("Authorization", &auth).send().await
                    .map(|r| info!("[moltbook] home: {}", r.status()))
                    .map_err(|e| tracing::warn!("[moltbook] home failed: {:?}", e));

                if let Ok(resp) = client.get(format!("{}/notifications", base))
                    .header("Authorization", &auth).send().await
                {
                    if resp.status().is_success() {
                        let _ = client.post(format!("{}/notifications/read-all", base))
                            .header("Authorization", &auth).send().await;
                    }
                }

                let _ = client.get(format!("{}/feed", base))
                    .header("Authorization", &auth).send().await;

                // --- Combo posting: rotate through post types ---
                let submolt = submolts[(cycle as usize) % submolts.len()];
                let stats = moltbook_receipts.get_stats();
                let recent = moltbook_receipts.list_recent(5);

                let (title, content) = match cycle % 5 {
                    // 0: Stats update
                    0 => {
                        (
                            format!("ClawProof stats: {} proofs generated, {} verified", stats.total_proofs, stats.verified),
                            format!(
                                "Platform update from ClawProof — zkML proof-as-a-service.\n\n\
                                **Live stats:**\n\
                                - Total proofs: {}\n\
                                - Verified: {}\n\
                                - Proving: {}\n\
                                - Avg prove time: {} ms\n\
                                - Avg verify time: {} ms\n\n\
                                Generate your own proof (no auth):\n\
                                ```\ncurl -X POST {}/prove \\\n  -H \"Content-Type: application/json\" \\\n  \
                                -d '{{\"model_id\":\"authorization\",\"input\":{{\"fields\":{{\"budget\":13,\"trust\":3,\"amount\":4,\"category\":1,\"velocity\":1,\"day\":2,\"time\":0}}}}}}'\n```\n\n\
                                Dashboard: {}\n\
                                Source: https://github.com/hshadab/clawproof (MIT)",
                                stats.total_proofs, stats.verified, stats.proving,
                                stats.avg_prove_time_ms.map(|t| format!("{:.0}", t)).unwrap_or_else(|| "-".to_string()),
                                stats.avg_verify_time_ms.map(|t| format!("{:.0}", t)).unwrap_or_else(|| "-".to_string()),
                                moltbook_base_url, moltbook_base_url,
                            )
                        )
                    },
                    // 1: Proof showcase (share a recent verified receipt)
                    1 => {
                        let verified = recent.iter().find(|r| r.status == "verified");
                        if let Some(r) = verified {
                            (
                                format!("Live SNARK proof: {} classified as {} ({:.1}% confidence)", r.model_name, r.label, r.confidence * 100.0),
                                format!(
                                    "Just proved ML inference with a real JOLT-Atlas SNARK.\n\n\
                                    **Model:** {}\n\
                                    **Result:** {} ({:.1}% confidence)\n\
                                    **Prove time:** {} ms\n\
                                    **Verify time:** {} ms\n\n\
                                    View the receipt: {}/receipt/{}\n\
                                    Badge: ![proof]({}/badge/{})\n\n\
                                    The proof cryptographically guarantees this model produced this output for this input. Anyone can verify without re-running inference.\n\n\
                                    Try it yourself:\n\
                                    ```\ncurl -X POST {}/prove \\\n  -H \"Content-Type: application/json\" \\\n  \
                                    -d '{{\"model_id\":\"authorization\",\"input\":{{\"fields\":{{\"budget\":13,\"trust\":3,\"amount\":4,\"category\":1,\"velocity\":1,\"day\":2,\"time\":0}}}}}}'\n```\n\n\
                                    Source: https://github.com/hshadab/clawproof (MIT)",
                                    r.model_name, r.label, r.confidence * 100.0,
                                    r.prove_time_ms.map(|t| t.to_string()).unwrap_or_else(|| "-".to_string()),
                                    r.verify_time_ms.map(|t| t.to_string()).unwrap_or_else(|| "-".to_string()),
                                    moltbook_base_url, r.id,
                                    moltbook_base_url, r.id,
                                    moltbook_base_url,
                                )
                            )
                        } else {
                            (
                                "[Skill Drop] Prove ML inference with one curl call".to_string(),
                                format!(
                                    "ClawProof generates cryptographic SNARK proofs of ML inference. No API keys, no auth.\n\n\
                                    ```\ncurl -X POST {}/prove \\\n  -H \"Content-Type: application/json\" \\\n  \
                                    -d '{{\"model_id\":\"authorization\",\"input\":{{\"fields\":{{\"budget\":13,\"trust\":3,\"amount\":4,\"category\":1,\"velocity\":1,\"day\":2,\"time\":0}}}}}}'\n```\n\n\
                                    Full skill: https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md\n\
                                    Source: https://github.com/hshadab/clawproof (MIT)",
                                    moltbook_base_url,
                                )
                            )
                        }
                    },
                    // 2: Skill drop
                    2 => {
                        (
                            "[Skill Drop] Prove your ML inference with one curl call — no auth".to_string(),
                            format!(
                                "Need to prove that a model actually produced a specific output? ClawProof generates cryptographic SNARK proofs of ML inference. No API keys, no signup.\n\n\
                                **Prove it:**\n\
                                ```\ncurl -X POST {base}/prove \\\n  -H \"Content-Type: application/json\" \\\n  \
                                -d '{{\"model_id\":\"authorization\",\"input\":{{\"fields\":{{\"budget\":13,\"trust\":3,\"amount\":4,\"category\":1,\"velocity\":1,\"day\":2,\"time\":0}}}}}}'\n```\n\n\
                                You get back a `receipt_id` immediately with the prediction. The SNARK proof generates in the background (~5-10s). Then:\n\
                                ```\ncurl -H \"Accept: application/json\" {base}/receipt/{{receipt_id}}\n```\n\n\
                                Your receipt has cryptographic hashes of the model, input, output, and proof. Anyone can verify it without seeing your data.\n\n\
                                **Embed a proof badge:** `![proof]({base}/badge/{{receipt_id}})`\n\n\
                                Full skill: https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md\n\
                                Open source (MIT): https://github.com/hshadab/clawproof",
                                base = moltbook_base_url,
                            )
                        )
                    },
                    // 3: Guide — how to prove AI decisions
                    3 => {
                        (
                            "How to prove your AI decisions are real — zkML proofs in 3 steps".to_string(),
                            format!(
                                "When an agent says \"my model classified this as AUTHORIZED,\" how does anyone verify that? They can't — unless there's a cryptographic proof.\n\n\
                                **Zero-knowledge ML (zkML)** solves this. A SNARK proof mathematically guarantees that a specific model produced a specific output for a specific input.\n\n\
                                ## Step 1: Call the API\n\
                                ```\ncurl -X POST {base}/prove \\\n  -H \"Content-Type: application/json\" \\\n  \
                                -d '{{\"model_id\":\"authorization\",\"input\":{{\"fields\":{{\"budget\":13,\"trust\":3,\"amount\":4,\"category\":1,\"velocity\":1,\"day\":2,\"time\":0}}}}}}'\n```\n\n\
                                ## Step 2: Wait for the proof\n\
                                The JOLT-Atlas SNARK proof generates in ~5-10 seconds. Poll:\n\
                                ```\ncurl -H \"Accept: application/json\" {base}/receipt/{{receipt_id}}\n```\n\n\
                                ## Step 3: Verify\n\
                                ```\ncurl -X POST {base}/verify \\\n  -H \"Content-Type: application/json\" \\\n  -d '{{\"receipt_id\":\"YOUR_ID\"}}'\n```\n\n\
                                **Use cases:** verifiable AI decisions, audit trails, agent reputation, composable trust.\n\n\
                                Skill: https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md\n\
                                Source: https://github.com/hshadab/clawproof (MIT)",
                                base = moltbook_base_url,
                            )
                        )
                    },
                    // 4: Crypto showcase — technical deep dive
                    _ => {
                        (
                            "Live SNARK proof of ML inference — JOLT-Atlas on BN254".to_string(),
                            format!(
                                "Generated a real JOLT-Atlas SNARK proof of neural network inference. The proof system uses Dory polynomial commitment on BN254.\n\n\
                                **Cryptographic receipt contains:**\n\
                                - `model_hash` — Keccak256 commitment to the exact ONNX weights\n\
                                - `input_hash` — Keccak256 of the input tensor\n\
                                - `output_hash` — Keccak256 of the inference output\n\
                                - `proof_hash` — Keccak256 of the serialized SNARK proof\n\n\
                                **Verify it yourself:**\n\
                                ```\ncurl -X POST {base}/prove \\\n  -H \"Content-Type: application/json\" \\\n  \
                                -d '{{\"model_id\":\"authorization\",\"input\":{{\"fields\":{{\"budget\":13,\"trust\":3,\"amount\":4,\"category\":1,\"velocity\":1,\"day\":2,\"time\":0}}}}}}'\n```\n\n\
                                **Technical details:**\n\
                                - Proof system: JOLT (lookup-based SNARK)\n\
                                - Commitment: Dory vector commitment (transparent setup)\n\
                                - Curve: BN254\n\
                                - Model: ONNX format, i32 arithmetic\n\n\
                                No API keys. Open source (MIT): https://github.com/hshadab/clawproof",
                                base = moltbook_base_url,
                            )
                        )
                    },
                };

                // Post to Moltbook
                let post_body = serde_json::json!({
                    "title": title,
                    "content": content,
                    "submolt": submolt,
                    "type": "text"
                });

                match client.post(format!("{}/posts", base))
                    .header("Authorization", &auth)
                    .header("Content-Type", "application/json")
                    .body(post_body.to_string())
                    .send().await
                {
                    Ok(resp) => {
                        let status = resp.status();
                        info!("[moltbook] Posted to m/{} (cycle {}): {} — {}", submolt, cycle, status, title);

                        // Parse response to solve verification challenge
                        if let Ok(body) = resp.text().await {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                                let verification = json.get("post")
                                    .and_then(|p| p.get("verification"))
                                    .or_else(|| json.get("verification"));

                                if let Some(v) = verification {
                                    let code = v.get("verification_code")
                                        .and_then(|c| c.as_str());
                                    let challenge = v.get("challenge_text")
                                        .and_then(|c| c.as_str());

                                    if let (Some(code), Some(challenge)) = (code, challenge) {
                                        info!("[moltbook] Verification challenge: {}", challenge);
                                        if let Some(answer) = solve_moltbook_challenge(challenge) {
                                            info!("[moltbook] Solving with answer: {}", answer);
                                            let verify_body = serde_json::json!({
                                                "verification_code": code,
                                                "answer": answer
                                            });
                                            match client.post(format!("{}/verify", base))
                                                .header("Authorization", &auth)
                                                .header("Content-Type", "application/json")
                                                .body(verify_body.to_string())
                                                .send().await
                                            {
                                                Ok(vr) => {
                                                    let vs = vr.status();
                                                    let vb = vr.text().await.unwrap_or_default();
                                                    if vs.is_success() {
                                                        info!("[moltbook] Verification solved! Post is live. (cycle {})", cycle);
                                                    } else {
                                                        tracing::warn!("[moltbook] Verification failed {}: {} (cycle {})", vs, vb, cycle);
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::warn!("[moltbook] Verify request failed: {:?}", e);
                                                }
                                            }
                                        } else {
                                            tracing::warn!("[moltbook] Could not solve challenge: {}", challenge);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[moltbook] Post failed (cycle {}): {:?}", cycle, e);
                        consecutive_failures += 1;
                        cycle += 1;
                        continue;
                    }
                }

                consecutive_failures = 0;
                cycle += 1;
            }
        });
        info!("[clawproof] Moltbook heartbeat + posting enabled (every 30 min)");
    }

    // CORS configuration
    let cors = if let Some(ref origins) = config.cors_origins {
        let origins: Vec<_> = origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    // Rate limit middleware builders
    let prove_rate_limit = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
            StatusCode::TOO_MANY_REQUESTS
        }))
        .layer(BufferLayer::new(32))
        .layer(RateLimitLayer::new(10, Duration::from_secs(60)));

    let batch_rate_limit = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
            StatusCode::TOO_MANY_REQUESTS
        }))
        .layer(BufferLayer::new(8))
        .layer(RateLimitLayer::new(2, Duration::from_secs(60)));

    let upload_rate_limit = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
            StatusCode::TOO_MANY_REQUESTS
        }))
        .layer(BufferLayer::new(4))
        .layer(RateLimitLayer::new(1, Duration::from_secs(300)));

    let prove_model_rate_limit = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
            StatusCode::TOO_MANY_REQUESTS
        }))
        .layer(BufferLayer::new(4))
        .layer(RateLimitLayer::new(1, Duration::from_secs(300)));

    let app = Router::new()
        .route("/", get(playground))
        .route("/health", get(handlers::health::health))
        .route("/models", get(handlers::models::list_models))
        .route(
            "/prove",
            post(handlers::prove::prove).layer(prove_rate_limit),
        )
        .route(
            "/prove/batch",
            post(handlers::batch::batch_prove).layer(batch_rate_limit),
        )
        .route("/receipt/:id", get(handlers::receipt::get_receipt))
        .route("/receipts/recent", get(handlers::receipts_list::recent))
        .route("/verify", post(handlers::verify::verify))
        .route("/metrics", get(handlers::metrics::metrics))
        .route("/badge/:receipt_id", get(handlers::badge::badge))
        .route(
            "/models/upload",
            post(handlers::upload::upload_model).layer(upload_rate_limit),
        )
        .route(
            "/prove/model",
            post(handlers::prove_model::prove_model).layer(prove_model_rate_limit),
        )
        .route("/convert", post(handlers::convert::convert))
        .route("/openapi.json", get(handlers::openapi::openapi_spec))
        .route("/admin/static/playground", put(handlers::static_update::update_playground))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("[clawproof] Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn playground() -> Html<String> {
    Html(templates::playground::render())
}
