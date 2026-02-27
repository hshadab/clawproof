use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

/// Serializable struct for safe TOML generation (prevents format-string injection).
#[derive(Serialize)]
pub struct ModelTomlOutput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_type: String,
    pub input_dim: usize,
    pub input_shape: Vec<usize>,
    pub labels: Vec<String>,
    pub trace_length: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Text,
    StructuredFields,
    Raw,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub description: String,
    pub min: usize,
    pub max: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_type: InputType,
    pub input_dim: usize,
    pub input_shape: Vec<usize>,
    pub labels: Vec<String>,
    pub trace_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FieldSchema>>,
    /// Cached Keccak256 hash of the ONNX file, computed once during scan/upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_hash: Option<String>,
}

#[derive(Deserialize)]
pub struct ModelToml {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub input_type: String,
    pub input_dim: usize,
    #[serde(default)]
    pub input_shape: Vec<usize>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default = "default_trace_length")]
    pub trace_length: usize,
    #[serde(default)]
    pub fields: Vec<FieldToml>,
}

fn default_trace_length() -> usize {
    1 << 14
}

#[derive(Deserialize)]
pub struct FieldToml {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub min: usize,
    #[serde(default)]
    pub max: usize,
}

#[derive(Clone)]
pub struct ModelRegistry {
    models: HashMap<String, ModelDescriptor>,
    order: Vec<String>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn register(&mut self, model: ModelDescriptor) {
        if !self.order.contains(&model.id) {
            self.order.push(model.id.clone());
        }
        self.models.insert(model.id.clone(), model);
    }

    pub fn get(&self, id: &str) -> Option<&ModelDescriptor> {
        self.models.get(id)
    }

    pub fn list(&self) -> Vec<&ModelDescriptor> {
        self.order
            .iter()
            .filter_map(|id| self.models.get(id))
            .collect()
    }

    pub fn load_from_toml(path: &Path) -> Option<ModelDescriptor> {
        let contents = std::fs::read_to_string(path).ok()?;
        let toml_model: ModelToml = toml::from_str(&contents).ok()?;

        let input_type = match toml_model.input_type.as_str() {
            "text" => InputType::Text,
            "structured_fields" => InputType::StructuredFields,
            "raw" => InputType::Raw,
            _ => {
                warn!("[clawproof] Unknown input_type '{}' in {:?}", toml_model.input_type, path);
                return None;
            }
        };

        let input_shape = if toml_model.input_shape.is_empty() {
            vec![1, toml_model.input_dim]
        } else {
            toml_model.input_shape
        };

        let fields = if toml_model.fields.is_empty() {
            None
        } else {
            Some(
                toml_model
                    .fields
                    .into_iter()
                    .map(|f| FieldSchema {
                        name: f.name,
                        description: f.description,
                        min: f.min,
                        max: f.max,
                    })
                    .collect(),
            )
        };

        Some(ModelDescriptor {
            id: toml_model.id,
            name: toml_model.name,
            description: toml_model.description,
            input_type,
            input_dim: toml_model.input_dim,
            input_shape,
            labels: toml_model.labels,
            trace_length: toml_model.trace_length,
            fields,
            model_hash: None,
        })
    }

    pub fn scan_directory(&mut self, base: &Path) {
        if !base.exists() {
            return;
        }
        let entries = match std::fs::read_dir(base) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let toml_path = path.join("model.toml");
                let onnx_path = path.join("network.onnx");
                if toml_path.exists() && onnx_path.exists() {
                    if let Some(mut descriptor) = Self::load_from_toml(&toml_path) {
                        if !self.models.contains_key(&descriptor.id) {
                            // Pre-compute model hash from the ONNX file
                            if let Ok(bytes) = std::fs::read(&onnx_path) {
                                let hash = format!("0x{}", hex::encode(Keccak256::digest(&bytes)));
                                descriptor.model_hash = Some(hash);
                            }
                            info!("[clawproof] Loaded uploaded model: {}", descriptor.id);
                            self.register(descriptor);
                        }
                    }
                }
            }
        }
    }
}
