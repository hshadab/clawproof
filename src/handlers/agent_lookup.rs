use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct AgentLookupRequest {
    /// Moltbook agent URL (e.g. "https://www.moltbook.com/u/cybercentry")
    /// or just the agent name (e.g. "cybercentry").
    pub agent: String,
    /// Interaction type to score: "post", "comment", "dm", or "trade".
    /// Defaults to "comment".
    #[serde(default = "default_interaction")]
    pub interaction: String,
}

fn default_interaction() -> String {
    "comment".to_string()
}

#[derive(Serialize)]
pub struct AgentLookupResponse {
    pub agent_name: String,
    pub fields: AgentTrustFields,
    pub raw: AgentRawData,
}

#[derive(Serialize)]
pub struct AgentTrustFields {
    pub karma: u32,
    pub account_age: u32,
    pub follower_ratio: u32,
    pub post_frequency: u32,
    pub verification: u32,
    pub content_similarity: u32,
    pub interaction_type: u32,
}

#[derive(Serialize)]
pub struct AgentRawData {
    pub karma: i64,
    pub follower_count: i64,
    pub following_count: i64,
    pub posts: i64,
    pub comments: i64,
    pub days_old: f64,
    pub x_verified: bool,
}

/// Moltbook API response (subset of fields we need).
#[derive(Deserialize)]
struct MoltbookProfile {
    #[serde(default)]
    karma: i64,
    #[serde(default)]
    follower_count: i64,
    #[serde(default)]
    following_count: i64,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    stats: Option<MoltbookStats>,
    #[serde(default)]
    owner: Option<MoltbookOwner>,
}

#[derive(Deserialize, Default)]
struct MoltbookStats {
    #[serde(default)]
    posts: i64,
    #[serde(default)]
    comments: i64,
}

#[derive(Deserialize, Default)]
struct MoltbookOwner {
    #[serde(default)]
    x_verified: bool,
}

/// Extract agent name from a URL like "https://www.moltbook.com/u/foo" or just "foo".
fn parse_agent_name(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches('/');

    // Try to parse as URL with /u/ path
    if let Some(pos) = trimmed.find("/u/") {
        let name = &trimmed[pos + 3..];
        let name = name.split('/').next().unwrap_or(name);
        let name = name.split('?').next().unwrap_or(name);
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // Otherwise treat the whole thing as a username (no slashes, no spaces)
    let name = trimmed.split('/').last().unwrap_or(trimmed);
    if !name.is_empty() && !name.contains(' ') && !name.contains('.') {
        return Some(name.to_string());
    }

    None
}

/// Bucket karma (raw value) into 0-10.
fn bucket_karma(raw: i64) -> u32 {
    match raw {
        ..=0 => 0,
        1..=5 => 1,
        6..=15 => 2,
        16..=30 => 3,
        31..=60 => 4,
        61..=120 => 5,
        121..=250 => 6,
        251..=500 => 7,
        501..=1000 => 8,
        1001..=5000 => 9,
        _ => 10,
    }
}

/// Bucket account age (days) into 0-7.
fn bucket_account_age(days: f64) -> u32 {
    match days as i64 {
        ..=0 => 0,
        1..=3 => 1,
        4..=7 => 2,
        8..=14 => 3,
        15..=30 => 4,
        31..=90 => 5,
        91..=180 => 6,
        _ => 7,
    }
}

/// Bucket follower ratio (followers/following) into 0-5.
fn bucket_follower_ratio(followers: i64, following: i64) -> u32 {
    if following == 0 && followers == 0 {
        return 0;
    }
    let ratio = if following == 0 {
        5.0 // many followers, no following → high ratio
    } else {
        followers as f64 / following as f64
    };
    match ratio {
        r if r < 0.1 => 0,
        r if r < 0.3 => 1,
        r if r < 0.7 => 2,
        r if r < 1.5 => 3,
        r if r < 3.0 => 4,
        _ => 5,
    }
}

/// Bucket post frequency (posts per day) into 0-5.
fn bucket_post_frequency(total_posts: i64, days: f64) -> u32 {
    if days <= 0.0 {
        return if total_posts > 0 { 5 } else { 0 };
    }
    let ppd = total_posts as f64 / days;
    match ppd {
        p if p < 0.1 => 0,
        p if p < 0.5 => 1,
        p if p < 1.5 => 2,
        p if p < 3.0 => 3,
        p if p < 8.0 => 4,
        _ => 5,
    }
}

/// Map interaction string to 0-3.
fn parse_interaction(s: &str) -> u32 {
    match s.to_lowercase().as_str() {
        "post" => 0,
        "comment" => 1,
        "dm" | "message" => 2,
        "trade" | "transaction" => 3,
        _ => 1, // default to comment
    }
}

pub async fn agent_lookup(
    State(state): State<AppState>,
    Json(req): Json<AgentLookupRequest>,
) -> Result<Json<AgentLookupResponse>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = state.config.moltbook_api_key.as_deref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Moltbook API key not configured"})),
        )
    })?;

    let agent_name = parse_agent_name(&req.agent).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid agent name or URL. Use a Moltbook URL like https://www.moltbook.com/u/agent-name or just the agent name."})),
        )
    })?;

    let client = reqwest::Client::new();
    let url = format!(
        "https://www.moltbook.com/api/v1/agents/profile?name={}",
        agent_name
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| {
            warn!("[clawproof] Moltbook API request failed: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "Failed to reach Moltbook API"})),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Agent '{}' not found on Moltbook (status {})", agent_name, status)
            })),
        ));
    }

    let profile: MoltbookProfile = resp.json().await.map_err(|e| {
        warn!("[clawproof] Failed to parse Moltbook profile: {}", e);
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": "Failed to parse Moltbook API response"})),
        )
    })?;

    // Compute days since account creation
    let days_old = profile
        .created_at
        .as_deref()
        .and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|created| {
                    let now = chrono::Utc::now();
                    (now - created.with_timezone(&chrono::Utc))
                        .num_seconds() as f64
                        / 86400.0
                })
        })
        .unwrap_or(0.0);

    let stats = profile.stats.unwrap_or_default();
    let owner = profile.owner.unwrap_or_default();

    let total_posts = stats.posts + stats.comments;

    let verification = if owner.x_verified { 2 } else { 0 };

    let fields = AgentTrustFields {
        karma: bucket_karma(profile.karma),
        account_age: bucket_account_age(days_old),
        follower_ratio: bucket_follower_ratio(profile.follower_count, profile.following_count),
        post_frequency: bucket_post_frequency(total_posts, days_old),
        verification,
        content_similarity: 0, // not derivable from profile data
        interaction_type: parse_interaction(&req.interaction),
    };

    let raw = AgentRawData {
        karma: profile.karma,
        follower_count: profile.follower_count,
        following_count: profile.following_count,
        posts: stats.posts,
        comments: stats.comments,
        days_old,
        x_verified: owner.x_verified,
    };

    Ok(Json(AgentLookupResponse {
        agent_name,
        fields,
        raw,
    }))
}
