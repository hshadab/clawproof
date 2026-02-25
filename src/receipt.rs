use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Proving,
    Verified,
    Failed,
}

impl ReceiptStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReceiptStatus::Proving => "proving",
            ReceiptStatus::Verified => "verified",
            ReceiptStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "verified" => ReceiptStatus::Verified,
            "failed" => ReceiptStatus::Failed,
            _ => ReceiptStatus::Proving,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceOutput {
    pub raw_output: Vec<i32>,
    pub predicted_class: usize,
    pub label: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub model_id: String,
    pub model_name: String,
    pub status: ReceiptStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,

    // Hashes
    pub model_hash: String,
    pub input_hash: String,
    pub output_hash: String,

    // Inference
    pub output: InferenceOutput,

    // Proof (populated after proving)
    pub proof_hash: Option<String>,
    pub proof_size: Option<usize>,
    pub prove_time_ms: Option<u128>,
    pub verify_time_ms: Option<u128>,

    // Error (if failed)
    pub error: Option<String>,
}

pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS receipts (
                id TEXT PRIMARY KEY,
                model_id TEXT NOT NULL,
                model_name TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                model_hash TEXT NOT NULL,
                input_hash TEXT NOT NULL,
                output_hash TEXT NOT NULL,
                output_json TEXT NOT NULL,
                proof_hash TEXT,
                proof_size INTEGER,
                prove_time_ms INTEGER,
                verify_time_ms INTEGER,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_receipts_status ON receipts(status);
            CREATE INDEX IF NOT EXISTS idx_receipts_model_id ON receipts(model_id);"
        )?;
        Ok(())
    }

    pub fn insert(&self, receipt: &Receipt) {
        let conn = self.conn.lock().unwrap();
        let output_json = serde_json::to_string(&receipt.output).unwrap_or_default();
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO receipts (id, model_id, model_name, status, created_at, completed_at, model_hash, input_hash, output_hash, output_json, proof_hash, proof_size, prove_time_ms, verify_time_ms, error) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                receipt.id,
                receipt.model_id,
                receipt.model_name,
                receipt.status.as_str(),
                receipt.created_at.to_rfc3339(),
                receipt.completed_at.map(|t| t.to_rfc3339()),
                receipt.model_hash,
                receipt.input_hash,
                receipt.output_hash,
                output_json,
                receipt.proof_hash,
                receipt.proof_size.map(|s| s as i64),
                receipt.prove_time_ms.map(|t| t as i64),
                receipt.verify_time_ms.map(|t| t as i64),
                receipt.error,
            ],
        ) {
            error!("[clawproof] SQLite insert failed: {:?}", e);
        }
    }

    pub fn get(&self, id: &str) -> Option<Receipt> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, model_id, model_name, status, created_at, completed_at, model_hash, input_hash, output_hash, output_json, proof_hash, proof_size, prove_time_ms, verify_time_ms, error FROM receipts WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let status_str: String = row.get(3)?;
                let created_str: String = row.get(4)?;
                let completed_str: Option<String> = row.get(5)?;
                let output_json: String = row.get(9)?;
                let proof_size: Option<i64> = row.get(11)?;
                let prove_time: Option<i64> = row.get(12)?;
                let verify_time: Option<i64> = row.get(13)?;

                Ok(Receipt {
                    id: row.get(0)?,
                    model_id: row.get(1)?,
                    model_name: row.get(2)?,
                    status: ReceiptStatus::from_str(&status_str),
                    created_at: DateTime::parse_from_rfc3339(&created_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    completed_at: completed_str.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                    model_hash: row.get(6)?,
                    input_hash: row.get(7)?,
                    output_hash: row.get(8)?,
                    output: serde_json::from_str(&output_json).unwrap_or(InferenceOutput {
                        raw_output: vec![],
                        predicted_class: 0,
                        label: "unknown".to_string(),
                        confidence: 0.0,
                    }),
                    proof_hash: row.get(10)?,
                    proof_size: proof_size.map(|s| s as usize),
                    prove_time_ms: prove_time.map(|t| t as u128),
                    verify_time_ms: verify_time.map(|t| t as u128),
                    error: row.get(14)?,
                })
            },
        )
        .ok()
    }

    pub fn get_stats(&self) -> ReceiptStats {
        let conn = self.conn.lock().unwrap();
        let mut stats = ReceiptStats::default();

        // Total
        stats.total_proofs = conn
            .query_row("SELECT COUNT(*) FROM receipts", [], |row| row.get(0))
            .unwrap_or(0);

        // By status
        stats.verified = conn
            .query_row(
                "SELECT COUNT(*) FROM receipts WHERE status = 'verified'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        stats.failed = conn
            .query_row(
                "SELECT COUNT(*) FROM receipts WHERE status = 'failed'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        stats.proving = conn
            .query_row(
                "SELECT COUNT(*) FROM receipts WHERE status = 'proving'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // By model
        if let Ok(mut stmt) =
            conn.prepare("SELECT model_id, COUNT(*) FROM receipts GROUP BY model_id")
        {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            }) {
                for row in rows.flatten() {
                    stats.by_model.insert(row.0, row.1);
                }
            }
        }

        // Average times
        stats.avg_prove_time_ms = conn
            .query_row(
                "SELECT AVG(prove_time_ms) FROM receipts WHERE prove_time_ms IS NOT NULL",
                [],
                |row| row.get::<_, Option<f64>>(0),
            )
            .unwrap_or(None);
        stats.avg_verify_time_ms = conn
            .query_row(
                "SELECT AVG(verify_time_ms) FROM receipts WHERE verify_time_ms IS NOT NULL",
                [],
                |row| row.get::<_, Option<f64>>(0),
            )
            .unwrap_or(None);

        stats
    }
}

impl Clone for SqliteStore {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct ReceiptStats {
    pub total_proofs: u64,
    pub verified: u64,
    pub failed: u64,
    pub proving: u64,
    pub by_model: std::collections::HashMap<String, u64>,
    pub avg_prove_time_ms: Option<f64>,
    pub avg_verify_time_ms: Option<f64>,
}

#[derive(Clone)]
pub struct ReceiptStore {
    cache: Arc<DashMap<String, Receipt>>,
    db: SqliteStore,
}

impl ReceiptStore {
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        let db = SqliteStore::new(db_path)?;
        Ok(Self {
            cache: Arc::new(DashMap::new()),
            db,
        })
    }

    pub fn insert(&self, receipt: Receipt) {
        self.cache.insert(receipt.id.clone(), receipt.clone());
        let db = self.db.clone();
        let _ = tokio::task::spawn_blocking(move || {
            db.insert(&receipt);
        });
    }

    pub fn get(&self, id: &str) -> Option<Receipt> {
        // DashMap first (hot cache)
        if let Some(r) = self.cache.get(id) {
            return Some(r.value().clone());
        }
        // SQLite fallback
        let receipt = self.db.get(id)?;
        // Populate cache for future reads
        self.cache.insert(receipt.id.clone(), receipt.clone());
        Some(receipt)
    }

    pub fn update<F>(&self, id: &str, f: F)
    where
        F: FnOnce(&mut Receipt),
    {
        if let Some(mut entry) = self.cache.get_mut(id) {
            f(entry.value_mut());
            let receipt = entry.value().clone();
            let db = self.db.clone();
            let _ = tokio::task::spawn_blocking(move || {
                db.insert(&receipt);
            });
        }
    }

    pub fn cleanup_cache(&self, max_age: std::time::Duration) {
        let cutoff = Utc::now() - chrono::Duration::from_std(max_age).unwrap();
        let before = self.cache.len();
        self.cache.retain(|_, receipt| receipt.created_at > cutoff);
        let removed = before - self.cache.len();
        if removed > 0 {
            info!("[clawproof] Evicted {} receipts from cache", removed);
        }
    }

    pub fn get_stats(&self) -> ReceiptStats {
        self.db.get_stats()
    }
}
