use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    pub following_count: Option<i64>,
    pub posts: i64,
    pub comments: i64,
    pub days_old: f64,
    pub is_claimed: bool,
    pub x_verified: bool,
    pub content_spam_score: f64,
}

// ---------------------------------------------------------------------------
// Moltbook API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MoltbookProfile {
    #[serde(default)]
    karma: i64,
    #[serde(default)]
    follower_count: i64,
    #[serde(default)]
    following_count: Option<i64>,
    #[serde(default)]
    is_claimed: Option<bool>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    stats: Option<MoltbookStats>,
    #[serde(default)]
    owner: Option<MoltbookOwner>,
    #[serde(default, rename = "recentPosts")]
    recent_posts: Option<Vec<MoltbookPost>>,
    #[serde(default, rename = "recentComments")]
    recent_comments: Option<Vec<MoltbookComment>>,
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

#[derive(Deserialize)]
struct MoltbookPost {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Deserialize)]
struct MoltbookComment {
    #[serde(default)]
    body: Option<String>,
}

// ---------------------------------------------------------------------------
// URL / name parsing
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Bucketing functions
// ---------------------------------------------------------------------------

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
///
/// When `following` is `None` (field absent from API), we can't compute a
/// ratio — fall back to a moderate bucket (2) rather than assuming a
/// perfect ratio.
fn bucket_follower_ratio(followers: i64, following: Option<i64>) -> u32 {
    let following = match following {
        Some(f) => f,
        None => {
            // Field absent from API — use a neutral middle bucket
            return if followers > 0 { 2 } else { 0 };
        }
    };

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
        _ => 1,
    }
}

/// Derive verification level from profile data.
///   0 = unclaimed agent (no human has verified ownership)
///   1 = claimed (owner linked their account but X not verified)
///   2 = X-verified (owner's X/Twitter account is verified)
fn derive_verification(is_claimed: bool, x_verified: bool) -> u32 {
    if x_verified {
        2
    } else if is_claimed {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Content similarity / spam analysis
// ---------------------------------------------------------------------------

/// Analyze an agent's recent posts and comments for spam-like patterns.
///
/// Returns a raw score in 0.0..1.0 where higher = more spam-like.
/// Combines four signals:
///   1. Link density: ratio of posts containing URLs
///   2. Duplicate content: ratio of near-duplicate texts
///   3. Short-post ratio: fraction of very short posts (<30 chars)
///   4. Low vocabulary diversity: unique words / total words
fn compute_spam_score(
    recent_posts: &Option<Vec<MoltbookPost>>,
    recent_comments: &Option<Vec<MoltbookComment>>,
) -> f64 {
    // Collect all text bodies
    let mut texts: Vec<String> = Vec::new();

    if let Some(posts) = recent_posts {
        for p in posts {
            // Combine title + body for posts
            let mut text = String::new();
            if let Some(t) = &p.title {
                text.push_str(t);
                text.push(' ');
            }
            if let Some(b) = &p.body {
                text.push_str(b);
            }
            let text = text.trim().to_string();
            if !text.is_empty() {
                texts.push(text);
            }
        }
    }

    if let Some(comments) = recent_comments {
        for c in comments {
            if let Some(b) = &c.body {
                let text = b.trim().to_string();
                if !text.is_empty() {
                    texts.push(text);
                }
            }
        }
    }

    if texts.is_empty() {
        // No content to analyze — neutral (not spam, not clearly legit)
        return 0.0;
    }

    let n = texts.len() as f64;

    // Signal 1: Link density — what fraction of texts contain URLs
    let link_count = texts
        .iter()
        .filter(|t| t.contains("http://") || t.contains("https://"))
        .count() as f64;
    let link_ratio = link_count / n;

    // Signal 2: Duplicate content — normalize and deduplicate
    let normalized: Vec<String> = texts
        .iter()
        .map(|t| {
            t.to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let unique: HashSet<&str> = normalized.iter().map(|s| s.as_str()).collect();
    let duplicate_ratio = if normalized.len() > 1 {
        1.0 - (unique.len() as f64 / normalized.len() as f64)
    } else {
        0.0
    };

    // Signal 3: Short-post ratio — very short posts are often low-effort/spam
    let short_count = texts.iter().filter(|t| t.len() < 30).count() as f64;
    let short_ratio = short_count / n;

    // Signal 4: Vocabulary diversity — unique words / total words across all texts
    let all_text = texts.join(" ").to_lowercase();
    let words: Vec<&str> = all_text.split_whitespace().collect();
    let vocab_diversity = if words.len() > 5 {
        let unique_words: HashSet<&str> = words.iter().copied().collect();
        unique_words.len() as f64 / words.len() as f64
    } else {
        1.0 // too few words to judge
    };
    // Low diversity → higher spam score (invert: 1.0 - diversity)
    let low_diversity = 1.0 - vocab_diversity;

    // Weighted combination (tuned so typical spam patterns score high)
    let score = (link_ratio * 0.25)
        + (duplicate_ratio * 0.35)
        + (short_ratio * 0.15)
        + (low_diversity * 0.25);

    score.clamp(0.0, 1.0)
}

/// Bucket the raw spam score (0.0-1.0) into the model's 0-5 range.
fn bucket_content_similarity(spam_score: f64) -> u32 {
    match spam_score {
        s if s < 0.05 => 0,
        s if s < 0.15 => 1,
        s if s < 0.30 => 2,
        s if s < 0.50 => 3,
        s if s < 0.70 => 4,
        _ => 5,
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

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
    let is_claimed = profile.is_claimed.unwrap_or(false);
    let total_posts = stats.posts + stats.comments;

    // Derive verification: 0=unclaimed, 1=claimed, 2=X-verified
    let verification = derive_verification(is_claimed, owner.x_verified);

    // Analyze content for spam signals
    let spam_score = compute_spam_score(&profile.recent_posts, &profile.recent_comments);

    let fields = AgentTrustFields {
        karma: bucket_karma(profile.karma),
        account_age: bucket_account_age(days_old),
        follower_ratio: bucket_follower_ratio(profile.follower_count, profile.following_count),
        post_frequency: bucket_post_frequency(total_posts, days_old),
        verification,
        content_similarity: bucket_content_similarity(spam_score),
        interaction_type: parse_interaction(&req.interaction),
    };

    let raw = AgentRawData {
        karma: profile.karma,
        follower_count: profile.follower_count,
        following_count: profile.following_count,
        posts: stats.posts,
        comments: stats.comments,
        days_old,
        is_claimed,
        x_verified: owner.x_verified,
        content_spam_score: (spam_score * 1000.0).round() / 1000.0, // 3 decimal places
    };

    Ok(Json(AgentLookupResponse {
        agent_name,
        fields,
        raw,
    }))
}
