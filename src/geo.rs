use tracing::warn;

/// Look up city and country for an IP address using ip-api.com (free, 45 req/min, no key).
/// Returns (city, country) or (None, None) on failure. Never panics.
pub async fn lookup(client: &reqwest::Client, ip: &str) -> (Option<String>, Option<String>) {
    // Skip private/loopback IPs
    if ip.starts_with("127.")
        || ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip == "::1"
        || ip == "localhost"
    {
        return (None, None);
    }
    // Also skip 172.16-31.x.x
    if ip.starts_with("172.") {
        if let Some(second) = ip.split('.').nth(1).and_then(|s| s.parse::<u8>().ok()) {
            if (16..=31).contains(&second) {
                return (None, None);
            }
        }
    }

    let url = format!(
        "http://ip-api.com/json/{}?fields=status,city,country",
        ip
    );

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("[clawproof] Geo lookup failed for {}: {:?}", ip, e);
            return (None, None);
        }
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => {
            warn!("[clawproof] Geo lookup parse failed for {}: {:?}", ip, e);
            return (None, None);
        }
    };

    if json.get("status").and_then(|s| s.as_str()) != Some("success") {
        return (None, None);
    }

    let city = json.get("city").and_then(|c| c.as_str()).map(String::from);
    let country = json.get("country").and_then(|c| c.as_str()).map(String::from);

    (city, country)
}
