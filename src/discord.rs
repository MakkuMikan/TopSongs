use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::fs;

use crate::http_template::{apply_substitution, build_request_from_spec, build_vars_map, parse_http_spec};
use crate::net::send_with_debug;

#[derive(Debug, Deserialize)]
struct DiscordUser {
    bio: Option<String>,
}

pub async fn get_current_bio(token: &str, debug: bool) -> Result<String> {
    // Use default .http path exclusively
    let path = std::path::Path::new("http\\discord_get_me.http");
    let client = reqwest::Client::new();
    let resp = if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read .http file at {}", path.to_string_lossy()))?;
        let spec = parse_http_spec(&content)?;
        // Only substitute token or env vars; headers like UA/locale/etc must be hardcoded in the .http file
        let vars = build_vars_map(&[("DISCORD_TOKEN", token.to_string())]);
        let spec = apply_substitution(spec, &vars);
        let (rb, body_preview) = build_request_from_spec(&client, &spec)?;
        send_with_debug(rb, debug, body_preview).await?
    } else {
        // Required .http file missing
        return Err(anyhow!("Required http\\discord_get_me.http not found. Skipping Discord GET."));
    };
    let text = resp.text().await?;
    let user: DiscordUser = serde_json::from_str(&text)
        .context("Failed to parse Discord user profile JSON")?;
    Ok(user.bio.unwrap_or_default())
}

pub async fn update_bio(token: &str, new_bio: &str, debug: bool) -> Result<()> {
    // Use default .http path exclusively
    let path = std::path::Path::new("http\\discord_patch_bio.http");
    let client = reqwest::Client::new();
    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read .http file at {}", path.to_string_lossy()))?;
        let spec = parse_http_spec(&content)?;
        // Only substitute token/new bio. All header values must be hardcoded in the .http file.
        // IMPORTANT: The .http template wraps {{NEW_BIO}} in quotes, so we must inject a JSON-escaped string content
        // without surrounding quotes. Use serde_json to produce a JSON string and strip the outer quotes.
        let json_escaped = serde_json::to_string(new_bio)
            .map(|s| s[1..s.len()-1].to_string())
            .unwrap_or_else(|_| new_bio.to_string());
        let vars = build_vars_map(&[("DISCORD_TOKEN", token.to_string()), ("NEW_BIO", json_escaped)]);
        let spec = apply_substitution(spec, &vars);
        let (rb, body_preview) = build_request_from_spec(&client, &spec)?;
        let _resp = send_with_debug(rb, debug, body_preview).await?;
        Ok(())
    } else {
        // Required .http file missing
        return Err(anyhow!("Required http\\discord_patch_bio.http not found. Skipping Discord PATCH."));
    }
}
