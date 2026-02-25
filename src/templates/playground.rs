use std::sync::OnceLock;

/// Compiled-in fallback so the binary works even without the static/ directory.
const FALLBACK: &str = include_str!("../../static/playground.html");

/// Directory to check for a live version of the file (set via STATIC_DIR env var).
fn static_dir() -> &'static Option<String> {
    static DIR: OnceLock<Option<String>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var("STATIC_DIR").ok())
}

pub fn render() -> String {
    // Try to read from disk first so the file can be updated without recompiling.
    if let Some(dir) = static_dir() {
        let path = std::path::Path::new(dir).join("playground.html");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            return contents;
        }
    }

    // Fall back to the version baked in at compile time.
    FALLBACK.to_string()
}
