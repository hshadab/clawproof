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
use axum::routing::{get, post};
use axum::Router;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use axum::error_handling::HandleErrorLayer;
use axum::http::StatusCode;
use tower::ServiceBuilder;
use tower::limit::RateLimitLayer;
use tower::buffer::BufferLayer;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::info;

use ark_bn254::Fr;
use jolt_core::poly::commitment::dory::DoryCommitmentScheme;
use jolt_core::transcripts::KeccakTranscript;
use onnx_tracer::model;
use zkml_jolt_core::jolt::JoltSNARK;

use crate::config::Config;
use crate::input::{load_onehot_vocab, load_tfidf_vocab};
use crate::models::{InputType, ModelRegistry};
use crate::receipt::ReceiptStore;
use crate::state::{AppState, PreprocessingCache, VocabData};

#[allow(clippy::upper_case_acronyms)]
type PCS = DoryCommitmentScheme;
type Snark = JoltSNARK<Fr, PCS, KeccakTranscript>;

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

    // Scan uploaded models directory for previously uploaded models
    registry.scan_directory(&config.uploaded_models_dir);

    // Load vocabularies
    info!("[clawproof] Loading vocabularies...");
    let mut vocabs = HashMap::new();
    for model_desc in registry.list() {
        let vocab_path = config.models_dir.join(&model_desc.id).join("vocab.json");
        match &model_desc.input_type {
            InputType::Text => {
                info!("[clawproof] Loading TF-IDF vocab for {}", model_desc.id);
                match load_tfidf_vocab(&vocab_path) {
                    Ok(vocab) => {
                        info!("[clawproof]   {} entries loaded", vocab.len());
                        vocabs.insert(model_desc.id.clone(), VocabData::TfIdf(vocab));
                    }
                    Err(e) => {
                        tracing::warn!("[clawproof] Failed to load vocab for {}: {:?}", model_desc.id, e);
                    }
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
            let reg = bg_state.registry.read().unwrap();
            reg.list().into_iter().cloned().collect()
        };
        for model_desc in model_list {
            let model_id = model_desc.id.clone();
            let model_path = {
                let default = bg_config.models_dir.join(&model_id).join("network.onnx");
                if default.exists() {
                    default
                } else {
                    bg_config.uploaded_models_dir.join(&model_id).join("network.onnx")
                }
            };

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
        let mut interval = tokio::time::interval(Duration::from_secs(600));
        loop {
            interval.tick().await;
            receipts_clone.cleanup_cache(Duration::from_secs(3600));
        }
    });

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
        .route("/receipt/{id}", get(handlers::receipt::get_receipt))
        .route("/verify", post(handlers::verify::verify))
        .route("/metrics", get(handlers::metrics::metrics))
        .route("/badge/{receipt_id}", get(handlers::badge::badge))
        .route(
            "/models/upload",
            post(handlers::upload::upload_model).layer(upload_rate_limit),
        )
        .route("/convert", post(handlers::convert::convert))
        .route("/openapi.json", get(handlers::openapi::openapi_spec))
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
