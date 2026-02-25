use std::path::PathBuf;
use tracing::{error, warn};

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub models_dir: PathBuf,
    pub base_url: String,
    pub database_path: PathBuf,
    pub cors_origins: Option<String>,
    pub uploaded_models_dir: PathBuf,
    pub converter_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let port: u16 = match std::env::var("PORT") {
            Ok(p) => p.parse().unwrap_or_else(|_| {
                warn!("[clawproof] Invalid PORT value, defaulting to 3000");
                3000
            }),
            Err(_) => 3000,
        };

        let models_dir = PathBuf::from(
            std::env::var("MODELS_DIR")
                .unwrap_or_else(|_| "./models".to_string()),
        );

        if !models_dir.exists() {
            error!("[clawproof] Models directory not found: {:?}", models_dir);
            std::process::exit(1);
        }

        let base_url = std::env::var("BASE_URL")
            .unwrap_or_else(|_| format!("http://localhost:{}", port));

        let database_path = PathBuf::from(
            std::env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "./data/clawproof.db".to_string()),
        );

        let cors_origins = std::env::var("CORS_ORIGINS").ok();

        let uploaded_models_dir = PathBuf::from(
            std::env::var("UPLOADED_MODELS_DIR")
                .unwrap_or_else(|_| "./data/models".to_string()),
        );

        let converter_url = std::env::var("CONVERTER_URL").ok();

        Self {
            port,
            models_dir,
            base_url,
            database_path,
            cors_origins,
            uploaded_models_dir,
            converter_url,
        }
    }
}
