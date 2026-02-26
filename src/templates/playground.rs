use std::sync::OnceLock;

const FALLBACK: &str = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>ClawProof</title></head><body><p>Static files not found. Set STATIC_DIR or place playground.html in ./static/</p></body></html>"#;

fn static_dir() -> &'static Option<String> {
    static DIR: OnceLock<Option<String>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var("STATIC_DIR").ok())
}

pub fn render() -> String {
    // Try STATIC_DIR first, then ./static/ as default
    let dirs_to_try: Vec<String> = match static_dir() {
        Some(d) => vec![d.clone(), "./static".to_string()],
        None => vec!["./static".to_string()],
    };
    for dir in dirs_to_try {
        let path = std::path::Path::new(&dir).join("playground.html");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            return contents;
        }
    }
    FALLBACK.to_string()
}
