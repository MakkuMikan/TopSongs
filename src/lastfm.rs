use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::fs;

use crate::http_template::{apply_substitution, build_request_from_spec, build_vars_map, parse_http_spec};
use crate::net::send_with_debug;

#[derive(Debug, Deserialize)]
pub struct TopTracksResponse {
    pub toptracks: TopTracks,
}

#[derive(Debug, Deserialize)]
pub struct TopTracks {
    pub track: Vec<Track>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Track {
    pub name: String,
    pub playcount: String,
    pub artist: Artist,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Artist {
    pub name: String,
}

pub async fn fetch_top_tracks(
    username: &str,
    api_key: &str,
    period: &str,
    limit: u32,
    debug: bool,
) -> Result<Vec<Track>> {
    // Use default .http path exclusively
    let path = std::path::Path::new("http\\lastfm_top_tracks.http");
    let client = reqwest::Client::new();
    let resp = if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read .http file at {}", path.to_string_lossy()))?;
        let spec = parse_http_spec(&content)?;
        let vars = build_vars_map(&[
            ("USERNAME", username.to_string()),
            ("API_KEY", api_key.to_string()),
            ("PERIOD", period.to_string()),
            ("LIMIT", limit.to_string()),
        ]);
        let spec = apply_substitution(spec, &vars);
        let (rb, body_preview) = build_request_from_spec(&client, &spec)?;
        send_with_debug(rb, debug, body_preview).await?
    } else {
        // Required .http file missing; do nothing by returning no tracks
        if debug { eprintln!("Missing http\\lastfm_top_tracks.http. Skipping Last.fm request."); }
        return Ok(vec![]);
    };

    // Last.fm sometimes returns error JSON; try to detect
    let text = resp.text().await?;
    if text.contains("\"error\"") {
        if debug {
            eprintln!("Last.fm error response body: {}", text);
        }
        return Err(anyhow!("Last.fm error response"));
    }

    let parsed: TopTracksResponse = serde_json::from_str(&text)
        .context("Failed to parse Last.fm top tracks JSON")?;

    Ok(parsed.toptracks.track)
}
