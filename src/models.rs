use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

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
        let mut registry = Self {
            models: HashMap::new(),
            order: Vec::new(),
        };

        // Register hardcoded defaults
        registry.register(ModelDescriptor {
            id: "authorization".to_string(),
            name: "Transaction Authorization".to_string(),
            description: "Determines whether a transaction should be authorized or denied based on budget, trust, amount, and other features.".to_string(),
            input_type: InputType::StructuredFields,
            input_dim: 64,
            input_shape: vec![1, 64],
            labels: vec![
                "AUTHORIZED".to_string(),
                "DENIED".to_string(),
            ],
            trace_length: 1 << 14,
            fields: Some(vec![
                FieldSchema { name: "budget".to_string(), description: "Budget level".to_string(), min: 0, max: 15 },
                FieldSchema { name: "trust".to_string(), description: "Trust score".to_string(), min: 0, max: 7 },
                FieldSchema { name: "amount".to_string(), description: "Transaction amount".to_string(), min: 0, max: 15 },
                FieldSchema { name: "category".to_string(), description: "Merchant category".to_string(), min: 0, max: 3 },
                FieldSchema { name: "velocity".to_string(), description: "Transaction velocity".to_string(), min: 0, max: 7 },
                FieldSchema { name: "day".to_string(), description: "Day of week".to_string(), min: 0, max: 7 },
                FieldSchema { name: "time".to_string(), description: "Time of day".to_string(), min: 0, max: 3 },
                FieldSchema { name: "risk".to_string(), description: "Risk level".to_string(), min: 0, max: 0 },
            ]),
        });

        registry
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
                    if let Some(descriptor) = Self::load_from_toml(&toml_path) {
                        if !self.models.contains_key(&descriptor.id) {
                            info!("[clawproof] Loaded uploaded model: {}", descriptor.id);
                            self.register(descriptor);
                        }
                    }
                }
            }
        }
    }
}
