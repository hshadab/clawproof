use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

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

    // Caller identification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_country: Option<String>,
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
        let conn = self.conn.lock().expect("SQLite connection lock poisoned");
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
            CREATE INDEX IF NOT EXISTS idx_receipts_model_id ON receipts(model_id);
            CREATE INDEX IF NOT EXISTS idx_receipts_created_at ON receipts(created_at DESC);"
        )?;

        // Idempotent migrations: add caller-identification columns
        for col in &[
            "client_ip TEXT",
            "user_agent TEXT",
            "geo_city TEXT",
            "geo_country TEXT",
        ] {
            let sql = format!("ALTER TABLE receipts ADD COLUMN {}", col);
            // Ignore "duplicate column" errors — column already exists
            let _ = conn.execute_batch(&sql);
        }

        // Backfill known caller data from Render logs (only touches rows with NULL client_ip)
        let backfill = [
            // 2026-02-27 receipt
            ("2026-02-27", "67.85.90.117", "Chrome/145", "New York City", "United States"),
            // 2026-03-04 receipt
            ("2026-03-04", "67.85.90.117", "curl/8.5.0", "New York City", "United States"),
            // 2026-03-08 (5 receipts)
            ("2026-03-08", "133.175.252.192", "curl/8.7.1", "Kyoto", "Japan"),
            // 2026-03-10 receipt
            ("2026-03-10", "47.79.255.119", "PowerShell/5.1 zh-CN", "Singapore", "Singapore"),
        ];
        for (date, ip, ua, city, country) in &backfill {
            let sql = "UPDATE receipts SET client_ip = ?1, user_agent = ?2, geo_city = ?3, geo_country = ?4 WHERE created_at LIKE ?5 AND client_ip IS NULL";
            let date_prefix = format!("{}%", date);
            let _ = conn.execute(
                sql,
                rusqlite::params![ip, ua, city, country, date_prefix],
            );
        }

        Ok(())
    }

    pub fn insert(&self, receipt: &Receipt) {
        let conn = self.conn.lock().expect("SQLite connection lock poisoned");
        let output_json = serde_json::to_string(&receipt.output).unwrap_or_default();
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO receipts (id, model_id, model_name, status, created_at, completed_at, model_hash, input_hash, output_hash, output_json, proof_hash, proof_size, prove_time_ms, verify_time_ms, error, client_ip, user_agent, geo_city, geo_country) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                receipt.client_ip,
                receipt.user_agent,
                receipt.geo_city,
                receipt.geo_country,
            ],
        ) {
            error!("[clawproof] SQLite insert failed: {:?}", e);
        }
    }

    pub fn get(&self, id: &str) -> Option<Receipt> {
        let conn = self.conn.lock().expect("SQLite connection lock poisoned");
        conn.query_row(
            "SELECT id, model_id, model_name, status, created_at, completed_at, model_hash, input_hash, output_hash, output_json, proof_hash, proof_size, prove_time_ms, verify_time_ms, error, client_ip, user_agent, geo_city, geo_country FROM receipts WHERE id = ?1",
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
                    client_ip: row.get(15)?,
                    user_agent: row.get(16)?,
                    geo_city: row.get(17)?,
                    geo_country: row.get(18)?,
                })
            },
        )
        .ok()
    }

    pub fn get_stats(&self) -> ReceiptStats {
        let conn = self.conn.lock().expect("SQLite connection lock poisoned");
        let mut stats = ReceiptStats::default();

        // Single query for counts and averages
        let _ = conn.query_row(
            "SELECT \
                COUNT(*), \
                SUM(CASE WHEN status = 'verified' THEN 1 ELSE 0 END), \
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), \
                SUM(CASE WHEN status = 'proving' THEN 1 ELSE 0 END), \
                AVG(CASE WHEN prove_time_ms IS NOT NULL THEN prove_time_ms END), \
                AVG(CASE WHEN verify_time_ms IS NOT NULL THEN verify_time_ms END) \
            FROM receipts",
            [],
            |row| {
                stats.total_proofs = row.get(0).unwrap_or(0);
                stats.verified = row.get(1).unwrap_or(0);
                stats.failed = row.get(2).unwrap_or(0);
                stats.proving = row.get(3).unwrap_or(0);
                stats.avg_prove_time_ms = row.get(4).unwrap_or(None);
                stats.avg_verify_time_ms = row.get(5).unwrap_or(None);
                Ok(())
            },
        );

        // By model (second query — GROUP BY)
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

        stats
    }

    pub fn list_recent(&self, limit: u64) -> Vec<ReceiptSummary> {
        let conn = self.conn.lock().expect("SQLite connection lock poisoned");
        let mut stmt = match conn.prepare(
            "SELECT id, model_id, model_name, status, created_at, output_json, prove_time_ms, verify_time_ms, client_ip, geo_city, geo_country FROM receipts ORDER BY created_at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("[clawproof] list_recent query failed: {:?}", e);
                return vec![];
            }
        };

        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            let status_str: String = row.get(3)?;
            let created_str: String = row.get(4)?;
            let output_json: String = row.get(5)?;
            let prove_time: Option<i64> = row.get(6)?;
            let verify_time: Option<i64> = row.get(7)?;

            let output: InferenceOutput = serde_json::from_str(&output_json).unwrap_or(InferenceOutput {
                raw_output: vec![],
                predicted_class: 0,
                label: "unknown".to_string(),
                confidence: 0.0,
            });

            Ok(ReceiptSummary {
                id: row.get(0)?,
                model_id: row.get(1)?,
                model_name: row.get(2)?,
                label: output.label,
                confidence: output.confidence,
                status: status_str,
                prove_time_ms: prove_time.map(|t| t as u128),
                verify_time_ms: verify_time.map(|t| t as u128),
                created_at: DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                client_ip: row.get(8)?,
                geo_city: row.get(9)?,
                geo_country: row.get(10)?,
            })
        });

        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(e) => {
                error!("[clawproof] list_recent rows failed: {:?}", e);
                vec![]
            }
        }
    }

    pub fn update_geo(&self, id: &str, city: Option<String>, country: Option<String>) {
        let conn = self.conn.lock().expect("SQLite connection lock poisoned");
        if let Err(e) = conn.execute(
            "UPDATE receipts SET geo_city = ?1, geo_country = ?2 WHERE id = ?3",
            rusqlite::params![city, country, id],
        ) {
            error!("[clawproof] SQLite update_geo failed for {}: {:?}", id, e);
        }
    }
}

impl Clone for SqliteStore {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptSummary {
    pub id: String,
    pub model_id: String,
    pub model_name: String,
    pub label: String,
    pub confidence: f64,
    pub status: String,
    pub prove_time_ms: Option<u128>,
    pub verify_time_ms: Option<u128>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_country: Option<String>,
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
        drop(tokio::task::spawn_blocking(move || {
            db.insert(&receipt);
        }));
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
            drop(tokio::task::spawn_blocking(move || {
                db.insert(&receipt);
            }));
        } else if let Some(mut receipt) = self.db.get(id) {
            // Receipt was evicted from cache — load from SQLite, apply mutation, write back
            f(&mut receipt);
            self.cache.insert(receipt.id.clone(), receipt.clone());
            let db = self.db.clone();
            drop(tokio::task::spawn_blocking(move || {
                db.insert(&receipt);
            }));
        } else {
            warn!("[clawproof] update called for unknown receipt {}", id);
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

    pub fn list_recent(&self, limit: u64) -> Vec<ReceiptSummary> {
        self.db.list_recent(limit)
    }

    pub fn update_geo(&self, id: &str, city: Option<String>, country: Option<String>) {
        // Update hot cache
        if let Some(mut entry) = self.cache.get_mut(id) {
            entry.value_mut().geo_city = city.clone();
            entry.value_mut().geo_country = country.clone();
        }
        // Update SQLite
        let db = self.db.clone();
        let id = id.to_string();
        drop(tokio::task::spawn_blocking(move || {
            db.update_geo(&id, city, country);
        }));
    }
}
