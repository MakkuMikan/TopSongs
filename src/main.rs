mod cli;
mod lastfm;
mod discord;
mod http_template;
mod net;
mod render;
mod text;
mod clipboard;
mod config;
mod ui;

use std::env;

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;

use crate::cli::Cli;
use crate::discord::{get_current_bio, update_bio};
use crate::lastfm::{fetch_top_tracks, Track};
use crate::render::{interpret_escapes, render_template};
use crate::text::{normalize_pattern, strip_title};
use crate::clipboard::copy_to_clipboard;
use crate::config::load_config;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle generating an example config and exit
    if cli.generate_config {
        match std::fs::write("topsongs.config.kdl", crate::config::EXAMPLE_KDL) {
            Ok(_) => {
                println!("Wrote example config to topsongs.config.kdl");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Failed to write example config: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Handle generating .http templates and exit
    if let Some(which) = &cli.generate_http {
        use std::path::Path;
        use std::io::Write;
        let http_dir = Path::new("http");
        if !http_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(http_dir) {
                eprintln!("Failed to create http directory: {}", e);
                std::process::exit(1);
            }
        }

        // Barebones templates (no personal info)
        let lastfm_content = "GET https://ws.audioscrobbler.com/2.0/?method=user.gettoptracks&user={{USERNAME}}&period={{PERIOD}}&api_key={{API_KEY}}&format=json&limit={{LIMIT}}\n";
        let discord_get_content = "GET https://discord.com/api/v10/users/@me\nAuthorization: {{DISCORD_TOKEN}}\n";
        let discord_patch_content = concat!(
            "PATCH https://discord.com/api/v9/users/@me/profile\n",
            "Content-Type: application/json\n",
            "Authorization: {{DISCORD_TOKEN}}\n",
            "\n",
            "{\n  \"bio\": \"{{NEW_BIO}}\"\n}\n",
        );

        let mut created_any = false;
        let want_all = which.eq("ALL") || which.eq_ignore_ascii_case("all");
        let targets: Vec<(&str, &str)> = if want_all {
            vec![
                ("lastfm_top_tracks.http", lastfm_content),
                ("discord_get_me.http", discord_get_content),
                ("discord_patch_bio.http", discord_patch_content),
            ]
        } else {
            let (name, content) = match which.as_str() {
                "lastfm_top_tracks" | "lastfm_top_tracks.http" => ("lastfm_top_tracks.http", lastfm_content),
                "discord_get_me" | "discord_get_me.http" => ("discord_get_me.http", discord_get_content),
                "discord_patch_bio" | "discord_patch_bio.http" => ("discord_patch_bio.http", discord_patch_content),
                other => {
                    eprintln!("Unknown template name: {}. Use one of: lastfm_top_tracks | discord_get_me | discord_patch_bio", other);
                    std::process::exit(1);
                }
            };
            vec![(name, content)]
        };

        for (fname, content) in targets {
            let path = http_dir.join(fname);
            if path.exists() {
                println!("Exists, not overwriting: {}", path.display());
                continue;
            }
            match std::fs::File::create(&path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(content.as_bytes()) {
                        eprintln!("Failed to write {}: {}", path.display(), e);
                        std::process::exit(1);
                    } else {
                        println!("Created {}", path.display());
                        created_any = true;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to create {}: {}", path.display(), e);
                    std::process::exit(1);
                }
            }
        }
        if !created_any {
            println!("No files created (all requested templates already exist).\nLocation: {}", http_dir.display());
        }
        return Ok(());
    }

    // Load optional config (KDL)
    let cfg = load_config();

    // Resolve API key: CLI > env > config
    let api_key = match cli
        .api_key
        .clone()
        .or_else(|| env::var("LASTFM_API_KEY").ok())
        .or_else(|| cfg.as_ref().and_then(|c| c.api_key.clone()))
    {
        Some(k) => k,
        None => {
            eprintln!("ERROR: Missing Last.fm API key. Pass --api-key, set LASTFM_API_KEY env var, or provide api_key in topsongs.config.kdl.");
            std::process::exit(2);
        }
    };

    // Resolve Last.fm username: CLI > config (no env fallback)
    let username = match cli.username.clone().or_else(|| cfg.as_ref().and_then(|c| c.username.clone())) {
        Some(u) => u,
        None => {
            eprintln!("ERROR: Missing Last.fm username. Pass --username or set username in topsongs.config.kdl.");
            std::process::exit(2);
        }
    };

    // Resolve other options with config overriding built-in defaults when CLI left them at defaults
    // period
    let mut period = cli.period;
    if let Some(pstr) = cfg.as_ref().and_then(|c| c.period.clone()) {
        if matches!(period, crate::cli::Period::Overall) {
            period = match pstr.as_str() {
                "overall" => crate::cli::Period::Overall,
                "7day" => crate::cli::Period::SevenDay,
                "1month" => crate::cli::Period::OneMonth,
                "3month" => crate::cli::Period::ThreeMonth,
                "6month" => crate::cli::Period::SixMonth,
                "12month" => crate::cli::Period::TwelveMonth,
                _ => period,
            };
        }
    }

    // numeric options
    let mut limit = cli.limit;
    if limit == 10 {
        if let Some(v) = cfg.as_ref().and_then(|c| c.limit) { limit = v; }
    }
    // Determine selection preference: CLI overrides config; if None, we'll use interactive selection
    let select_opt: Option<usize> = cli.select.or_else(|| cfg.as_ref().and_then(|c| c.select));

    // strings with defaults
    let mut format = cli.format.clone();
    if format == "  - {artist} - {track}" {
        if let Some(v) = cfg.as_ref().and_then(|c| c.format.clone()) { format = v; }
    }
    let mut join = cli.join.clone();
    if join == "\n" {
        if let Some(v) = cfg.as_ref().and_then(|c| c.join.clone()) { join = v; }
    }
    let mut prefix = cli.prefix.clone();
    if prefix.is_empty() {
        if let Some(v) = cfg.as_ref().and_then(|c| c.prefix.clone()) { prefix = v; }
    }
    let mut suffix = cli.suffix.clone();
    if suffix.is_empty() {
        if let Some(v) = cfg.as_ref().and_then(|c| c.suffix.clone()) { suffix = v; }
    }

    // booleans
    let mut strip_feat = cli.strip_feat;
    if !strip_feat {
        if let Some(v) = cfg.as_ref().and_then(|c| c.strip_feat) { strip_feat = v; }
    }
    let mut copy = cli.copy;
    if !copy {
        if let Some(v) = cfg.as_ref().and_then(|c| c.copy) { copy = v; }
    }
    let mut debug = cli.debug;
    if !debug {
        if let Some(v) = cfg.as_ref().and_then(|c| c.debug) { debug = v; }
    }

    let mut strip_feat_regex = cli.strip_feat_regex.clone();
    if strip_feat_regex.is_none() {
        if let Some(v) = cfg.as_ref().and_then(|c| c.strip_feat_regex.clone()) { strip_feat_regex = Some(v); }
    }

    let mut discord_bio_regex = cli.discord_bio_regex.clone();
    if discord_bio_regex == r"/\*\*[\w ]+\*\*:?\r?(\n[ \w-]+)+\n/" {
        if let Some(v) = cfg.as_ref().and_then(|c| c.discord_bio_regex.clone()) { discord_bio_regex = v; }
    }

    let mut update_discord = cli.update_discord;
    if !update_discord {
        if let Some(v) = cfg.as_ref().and_then(|c| c.update_discord) { update_discord = v; }
    }
    let mut discord_dry_run = cli.discord_dry_run;
    if !discord_dry_run {
        if let Some(v) = cfg.as_ref().and_then(|c| c.discord_dry_run) { discord_dry_run = v; }
    }

    // Resolve Discord token: CLI > env > config
    let discord_token_opt = cli
        .discord_token
        .clone()
        .or_else(|| env::var("DISCORD_TOKEN").ok())
        .or_else(|| cfg.as_ref().and_then(|c| c.discord_token.clone()));

    let tracks = fetch_top_tracks(
        &username,
        &api_key,
        period.as_api_value(),
        limit,
        debug,
    )
    .await
        .with_context(|| "Failed to fetch top tracks from Last.fm")?;

    if tracks.is_empty() {
        println!("No tracks found. Check username or try a different period.");
        return Ok(());
    }

    println!("Top {} tracks for '{}' (period: {}):", tracks.len(), username, period.as_api_value());
    for (idx, t) in tracks.iter().enumerate() {
        let pc = t.playcount.parse::<u32>().unwrap_or(0);
        println!("{:>2}. {} — {} ({} plays)", idx + 1, t.artist.name, t.name, pc);
    }

    // Selection: auto-select top N if provided; otherwise prompt interactively
    let chosen: Vec<&Track> = if let Some(mut n) = select_opt {
        if n == 0 { n = 1; }
        if n > tracks.len() { n = tracks.len(); }
        println!("\nAuto-selecting top {} track(s).", n);
        tracks.iter().take(n).collect()
    } else {
        let items: Vec<String> = tracks.iter().enumerate().map(|(i, t)| {
            let pc = t.playcount.parse::<u32>().unwrap_or(0);
            // Prefix with list index to aid selection
            format!("{:02}) {} — {} ({} plays)", i + 1, t.artist.name, t.name, pc)
        }).collect();
        // Use Cursive-based ordered selection (compact dialog) to preserve the order you pick items
        let indices = crate::ui::select_ordered_with_cursive(items)?;
        if indices.is_empty() {
            eprintln!("No tracks selected. Exiting without output.");
            return Ok(());
        }
        indices.into_iter().map(|i| &tracks[i]).collect()
    };

    let rendered: Vec<String> = chosen
        .into_iter()
        .map(|t| {
            let title = if strip_feat {
                strip_title(&t.name, strip_feat_regex.as_deref())
            } else {
                t.name.clone()
            };
            let mut temp = t.clone();
            temp.name = title;
            render_template(&format, &temp)
        })
        .collect();

    // Interpret backslash escape sequences in join/prefix/suffix so that, e.g., "\\n" becomes a real newline.
    let join_str = interpret_escapes(&join);
    let prefix_i = interpret_escapes(&prefix);
    let suffix_i = interpret_escapes(&suffix);

    let list = rendered.join(&join_str);
    let output = format!("{}{}{}", prefix_i, list, suffix_i);
    println!("\nYour Discord bio line:\n{}", output);

    if copy {
        if let Err(e) = copy_to_clipboard(&output) {
            eprintln!("Failed to copy to clipboard: {}", e);
        } else {
            println!("Copied to clipboard.");
        }
    }

    // Discord operations are executed only when explicitly requested.
    let do_discord = update_discord || discord_dry_run;
    if do_discord {
        if let Some(token) = discord_token_opt.as_deref() {
            match get_current_bio(token, debug).await {
                Ok(current_bio) => {
                    let pattern = normalize_pattern(&discord_bio_regex);
                    let re = match Regex::new(&pattern) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("Invalid regex for --discord-bio-regex: {}", e);
                            return Ok(());
                        }
                    };

                    if let Some(_m) = re.find(&current_bio) {
                        let replacement = format!("{}\n", output);
                        let new_bio = re.replace(&current_bio, replacement.as_str()).to_string();

                        if discord_dry_run {
                            println!("\n[Discord dry-run] Would update bio to:\n{}", new_bio);
                            println!("[Discord dry-run] No changes were sent to Discord.");
                        } else if update_discord {
                            if new_bio == current_bio {
                                println!("Discord bio is already up to date. No update sent.");
                            } else {
                                match update_bio(token, &new_bio, debug).await {
                                    Ok(()) => println!("Discord bio updated successfully."),
                                    Err(e) => eprintln!("Failed to update Discord bio: {}", e),
                                }
                            }
                        }
                    } else {
                        eprintln!("The provided regex did not match your current Discord bio. No update performed.");
                    }
                }
                Err(e) => eprintln!("Failed to fetch current Discord bio: {}", e),
            }
        } else {
            eprintln!("Discord operations requested but no token provided. Use --discord-token, set DISCORD_TOKEN, or provide discord_token in config.");
        }
    }

    Ok(())
}
