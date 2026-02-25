use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

static TOKENIZER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\w+|[^\w\s]").unwrap());

/// TF-IDF vocab: word -> (index, idf_scaled)
pub type TfIdfVocab = HashMap<String, (usize, i32)>;

/// One-hot vocab: feature_key -> index
pub type OneHotVocab = HashMap<String, usize>;

pub fn load_tfidf_vocab(path: &Path) -> anyhow::Result<TfIdfVocab> {
    let contents = std::fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&contents)?;
    let mut vocab = HashMap::new();

    if let Value::Object(map) = json {
        for (word, data) in map {
            if let (Some(index), Some(idf)) = (
                data.get("index").and_then(|v| v.as_u64()),
                data.get("idf").and_then(|v| v.as_f64()),
            ) {
                vocab.insert(word, (index as usize, (idf * 1000.0) as i32));
            }
        }
    }

    Ok(vocab)
}

pub fn load_onehot_vocab(path: &Path) -> anyhow::Result<OneHotVocab> {
    let contents = std::fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&contents)?;
    let mut vocab = HashMap::new();

    if let Some(Value::Object(map)) = json.get("vocab_mapping") {
        for (feature_key, data) in map {
            if let Some(index) = data.get("index").and_then(|v| v.as_u64()) {
                vocab.insert(feature_key.clone(), index as usize);
            }
        }
    }

    Ok(vocab)
}

pub fn build_tfidf_vector(text: &str, vocab: &TfIdfVocab, dim: usize) -> Vec<i32> {
    let mut vec = vec![0i32; dim];

    for cap in TOKENIZER.captures_iter(text) {
        let token = cap.get(0).unwrap().as_str().to_lowercase();
        if let Some(&(index, idf)) = vocab.get(&token) {
            if index < dim {
                vec[index] += idf;
            }
        }
    }

    vec
}

pub fn build_onehot_vector(
    fields: &HashMap<String, usize>,
    field_names: &[&str],
    vocab: &OneHotVocab,
    dim: usize,
) -> Vec<i32> {
    let mut vec = vec![0i32; dim];

    for &field_name in field_names {
        if let Some(&value) = fields.get(field_name) {
            let feature_key = format!("{}_{}", field_name, value);
            if let Some(&index) = vocab.get(&feature_key) {
                if index < dim {
                    vec[index] = 1;
                }
            }
        }
    }

    vec
}
