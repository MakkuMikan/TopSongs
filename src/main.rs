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

fn print_kdl_parse_errors(path: &std::path::Path, source: &str, err: &kdl::KdlError) {
    // Header
    eprintln!("Config file found at {} but failed to parse as KDL:", path.display());

    // Try to print structured diagnostics if available
    let mut printed_any = false;

    // Try common access pattern: a `diagnostics` field on the error
    #[allow(unused_variables)]
    {
        // SAFETY: If this compiles, we have access to diagnostics
        #[allow(dead_code)]
        struct _Check;
        // Use pattern that should compile with kdl 6.x
        // If the field/method doesn't exist, the fallback below will be used.
    }

    // Attempt via field access (kdl 6.x exposes `diagnostics: Vec<KdlDiagnostic>`)
    #[allow(unused_variables)]
    {
        // Try direct field access
        // If this compiles for kdl 6.5.0, it will use the embedded diagnostics
        let diags: &[kdl::KdlDiagnostic] = &err.diagnostics;
        printed_any = print_diags_from_slice(source, diags);
    }

    // Final fallback: print Display for the error
    if !printed_any {
        eprintln!("  {}", err);
    }
}

fn print_diags_from_slice(source: &str, diags: &[kdl::KdlDiagnostic]) -> bool {
    let mut any = false;
    for d in diags {
        any = true;
        let msg = d.message.as_deref().unwrap_or("KDL parse error");
        eprintln!("  - {}", msg);
        // Use the main span to compute location
        let start = d.span.offset();
        let (line_no, col_no, line_text) = byte_range_to_line_col(source, start);
        eprintln!("      at line {}, col {}", line_no, col_no);
        let display_line = line_text.replace('\t', " ");
        eprintln!("        {}", display_line);
        let mut caret = String::new();
        for _ in 1..col_no { caret.push(' '); }
        caret.push('^');
        eprintln!("        {}", caret);
        if let Some(label) = d.label.as_deref() { eprintln!("        note: {}", label); }
        if let Some(help) = d.help.as_deref() { eprintln!("    help: {}", help); }
    }
    any
}

fn byte_range_to_line_col(source: &str, byte_start: usize) -> (usize, usize, String) {
    let mut acc = 0usize;
    for (i, line) in source.split_inclusive(['\n', '\r']).enumerate() {
        let line_len = line.len();
        if acc + line_len > byte_start {
            // Found line
            let line_no = i + 1;
            let col_no = byte_start - acc + 1; // 1-based
            let clean = line.trim_end_matches(['\n', '\r']);
            return (line_no, col_no, clean.to_string());
        }
        acc += line_len;
    }
    // Default to end
    let last_line = source.lines().last().unwrap_or("").to_string();
    (source.lines().count().max(1), last_line.len().saturating_add(1), last_line)
}

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
        use std::fs;
        let dir = crate::config::config_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("Failed to create config directory {}: {}", dir.display(), e);
            std::process::exit(1);
        }
        let path = dir.join("topsongs.config.kdl");
        match fs::write(&path, crate::config::EXAMPLE_KDL) {
            Ok(_) => {
                println!("Wrote example config to {}", path.display());
                return Ok(());
            }
            Err(e) => {
                eprintln!("Failed to write example config to {}: {}", path.display(), e);
                std::process::exit(1);
            }
        }
    }

    // Handle generating .http templates and exit
    if let Some(which) = &cli.generate_http {
        use std::io::Write;
        let http_dir = crate::config::http_dir();
        if !http_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&http_dir) {
                eprintln!("Failed to create http directory {}: {}", http_dir.display(), e);
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
    let found_config_path = crate::config::find_config_path();
    let cfg = load_config();

    // If no config loaded, differentiate between not found vs found-but-invalid
    if cfg.is_none() {
        if let Some(p) = &found_config_path {
            // Try to surface a helpful error message
            use std::fs;
            match fs::read_to_string(p) {
                Ok(content) => {
                    match content.parse::<kdl::KdlDocument>() {
                        Ok(_) => {
                            eprintln!("Config file found at {}, but failed to interpret its contents. Please check KDL structure.", p.display());
                        }
                        Err(e) => {
                            // Parse and print concise KDL diagnostics instead of dumping the whole error
                            print_kdl_parse_errors(p, &content, &e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Config file found at {} but failed to read: {}", p.display(), e);
                }
            }
        } else if cli.debug {
            let locations = crate::config::config_search_locations();
            eprintln!("No config file found. Searched locations:");
            for p in locations {
                eprintln!("  - {}", p.display());
            }
        }
    }

    // Determine early debug flag from CLI or config
    let early_debug = if cli.debug { true } else { cfg.as_ref().and_then(|c| c.debug).unwrap_or(false) };

    // If debug is enabled, print the config values as read from file (not the resolved effective values)
    if early_debug {
        match &cfg {
            Some(c) => {
                fn mask_opt(s: &Option<String>) -> String {
                    match s {
                        Some(v) if !v.is_empty() => {
                            if v.len() <= 4 { "****".to_string() } else { format!("{}***", &v[..2]) }
                        }
                        Some(_) => "".to_string(),
                        None => "<none>".to_string(),
                    }
                }
                println!("[debug] Config loaded (raw values as read):");
                println!("  username: {}", c.username.clone().unwrap_or_else(|| "<none>".into()));
                println!("  api_key: {}", mask_opt(&c.api_key));
                println!("  period: {}", c.period.clone().unwrap_or_else(|| "<none>".into()));
                println!("  limit: {}", c.limit.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
                println!("  select: {}", c.select.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
                println!("  format: {}", c.format.clone().unwrap_or_else(|| "<none>".into()));
                println!("  join: {}", c.join.clone().unwrap_or_else(|| "<none>".into()));
                println!("  prefix: {}", c.prefix.clone().unwrap_or_else(|| "<none>".into()));
                println!("  suffix: {}", c.suffix.clone().unwrap_or_else(|| "<none>".into()));
                println!("  strip_feat: {}", c.strip_feat.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
                println!("  strip_feat_regex: {}", c.strip_feat_regex.clone().unwrap_or_else(|| "<none>".into()));
                println!("  copy: {}", c.copy.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
                println!("  discord_token: {}", mask_opt(&c.discord_token));
                println!("  discord_bio_regex: {}", c.discord_bio_regex.clone().unwrap_or_else(|| "<none>".into()));
                println!("  update_discord: {}", c.update_discord.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
                println!("  discord_dry_run: {}", c.discord_dry_run.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
                println!("  debug: {}", c.debug.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()));
            }
            None => {
                if let Some(p) = &found_config_path {
                    println!("[debug] Config file was found at {} but failed to load (read/parse error). See error above.", p.display());
                } else {
                    println!("[debug] No config file was loaded (using CLI/env defaults)");
                    let locations = crate::config::config_search_locations();
                    for p in locations {
                        println!("[debug]   searched: {}", p.display());
                    }
                }
            }
        }
    }

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
