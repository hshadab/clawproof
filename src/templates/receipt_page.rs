use std::sync::OnceLock;
use crate::receipt::Receipt;

const FALLBACK: &str = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Receipt</title></head><body><p>Static files not found. Set STATIC_DIR or place receipt.html in ./static/</p></body></html>"#;

/// Directory to check for a live version of the file (set via STATIC_DIR env var).
fn static_dir() -> &'static Option<String> {
    static DIR: OnceLock<Option<String>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var("STATIC_DIR").ok())
}

fn load_template() -> String {
    // Try STATIC_DIR first, then ./static/ as default
    let dirs_to_try: Vec<String> = match static_dir() {
        Some(d) => vec![d.clone(), "./static".to_string()],
        None => vec!["./static".to_string()],
    };
    for dir in dirs_to_try {
        let path = std::path::Path::new(&dir).join("receipt.html");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            return contents;
        }
    }
    FALLBACK.to_string()
}

/// Render the receipt page by loading the static HTML template and injecting
/// only the OG meta tag values (for social media crawlers that don't run JS).
/// All other rendering happens client-side.
pub fn render(receipt: &Receipt, base_url: &str) -> String {
    let template = load_template();

    let og_title = format!(
        "ClawProof \u{2014} {} ({:.1}%)",
        receipt.output.label,
        receipt.output.confidence * 100.0,
    );
    let og_description = format!(
        "Cryptographically verified ML inference. Model: {}. Result: {} ({:.1}% confidence). Status: {}.",
        receipt.model_name,
        receipt.output.label,
        receipt.output.confidence * 100.0,
        receipt.status.as_str(),
    );
    let og_url = format!("{}/receipt/{}", base_url, receipt.id);
    let og_image = format!("{}/badge/{}", base_url, receipt.id);

    template
        .replace("{{OG_TITLE}}", &og_title)
        .replace("{{OG_DESCRIPTION}}", &og_description)
        .replace("{{OG_URL}}", &og_url)
        .replace("{{OG_IMAGE}}", &og_image)
}
