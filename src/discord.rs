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
    // Prefer config http dir; fall back to legacy ./http
    let preferred = crate::config::http_dir().join("discord_get_me.http");
    let legacy = std::path::Path::new("http\\discord_get_me.http").to_path_buf();
    let chosen = if preferred.exists() { preferred } else { legacy };
    let client = reqwest::Client::new();
    let resp = if chosen.exists() {
        let content = fs::read_to_string(&chosen)
            .with_context(|| format!("Failed to read .http file at {}", chosen.to_string_lossy()))?;
        let spec = parse_http_spec(&content)?;
        // Only substitute token or env vars; headers like UA/locale/etc must be hardcoded in the .http file
        let vars = build_vars_map(&[("DISCORD_TOKEN", token.to_string())]);
        let spec = apply_substitution(spec, &vars);
        let (rb, body_preview) = build_request_from_spec(&client, &spec)?;
        send_with_debug(rb, debug, body_preview).await?
    } else {
        // Required .http file missing
        return Err(anyhow!(
            format!(
                "Required discord_get_me.http not found in {} or legacy ./http. Run with --generate-http to create templates.",
                crate::config::http_dir().display()
            )
        ));
    };
    let text = resp.text().await?;
    let user: DiscordUser = serde_json::from_str(&text)
        .context("Failed to parse Discord user profile JSON")?;
    Ok(user.bio.unwrap_or_default())
}

pub async fn update_bio(token: &str, new_bio: &str, debug: bool) -> Result<()> {
    // Prefer config http dir; fall back to legacy ./http
    let preferred = crate::config::http_dir().join("discord_patch_bio.http");
    let legacy = std::path::Path::new("http\\discord_patch_bio.http").to_path_buf();
    let chosen = if preferred.exists() { preferred } else { legacy };
    let client = reqwest::Client::new();
    if chosen.exists() {
        let content = fs::read_to_string(&chosen)
            .with_context(|| format!("Failed to read .http file at {}", chosen.to_string_lossy()))?;
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
        return Err(anyhow!(
            format!(
                "Required discord_patch_bio.http not found in {} or legacy ./http. Run with --generate-http to create templates.",
                crate::config::http_dir().display()
            )
        ));
    }
}
