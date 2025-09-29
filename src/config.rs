use std::fs;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub username: Option<String>,
    pub api_key: Option<String>,
    pub period: Option<String>,
    pub limit: Option<u32>,
    pub select: Option<usize>,
    pub format: Option<String>,
    pub join: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub strip_feat: Option<bool>,
    pub strip_feat_regex: Option<String>,
    pub copy: Option<bool>,
    pub discord_token: Option<String>,
    pub discord_bio_regex: Option<String>,
    pub update_discord: Option<bool>,
    pub discord_dry_run: Option<bool>,
    pub debug: Option<bool>,
}

fn get_string(node: &kdl::KdlNode) -> Option<String> {
    node.entries().get(0)?.value().as_string().map(|s| s.to_string())
}
fn get_bool(node: &kdl::KdlNode) -> Option<bool> {
    node.entries().get(0)?.value().as_bool()
}
fn get_u32(node: &kdl::KdlNode) -> Option<u32> {
    node.entries().get(0)?.value().as_integer().and_then(|v| u32::try_from(v).ok())
}
fn get_usize(node: &kdl::KdlNode) -> Option<usize> {
    node.entries().get(0)?.value().as_integer().and_then(|v| usize::try_from(v).ok())
}

pub fn load_config() -> Option<Config> {
    // Look for a KDL config file in the current working directory
    let path = std::path::Path::new("topsongs.config.kdl");
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let doc: kdl::KdlDocument = content.parse().ok()?;

    // Support either a root node `topsongs { ... }` or flat top-level entries
    let node_span;
    let nodes: Vec<kdl::KdlNode> = if let Some(n) = doc.get("topsongs") {
        node_span = n.children().cloned();
        if let Some(children) = node_span { children.nodes().into_iter().cloned().collect() } else { vec![] }
    } else {
        doc.nodes().into_iter().cloned().collect()
    };

    let mut cfg = Config::default();
    for n in nodes {
        match n.name().value() {
            "username" => cfg.username = get_string(&n),
            "api_key" => cfg.api_key = get_string(&n),
            "period" => cfg.period = get_string(&n),
            "limit" => cfg.limit = get_u32(&n),
            "select" => cfg.select = get_usize(&n),
            "format" => cfg.format = get_string(&n),
            "join" => cfg.join = get_string(&n),
            "prefix" => cfg.prefix = get_string(&n),
            "suffix" => cfg.suffix = get_string(&n),
            "strip_feat" => cfg.strip_feat = get_bool(&n),
            "strip_feat_regex" => cfg.strip_feat_regex = get_string(&n),
            "copy" => cfg.copy = get_bool(&n),
            "discord_token" => cfg.discord_token = get_string(&n),
            "discord_bio_regex" => cfg.discord_bio_regex = get_string(&n),
            "update_discord" => cfg.update_discord = get_bool(&n),
            "discord_dry_run" => cfg.discord_dry_run = get_bool(&n),
            "debug" => cfg.debug = get_bool(&n),
            _ => {}
        }
    }
    Some(cfg)
}


// Example KDL configuration embedded here for convenience
pub const EXAMPLE_KDL: &str = r#"// topsongs.config.kdl
// You can wrap settings inside a `topsongs { ... }` block or keep them flat at the root.
// Strings should be quoted; numbers and booleans are bare.
// Note: To create barebones .http templates, run: topsongs --generate-http
//   - With no value: creates all missing default templates in ./http
//   - With a value: creates a specific one if missing (one of: lastfm_top_tracks | discord_get_me | discord_patch_bio)

// Escape sequences: `\n` becomes a newline in prefix/suffix/join and inside format.
// Selection: omit `select` to choose tracks interactively; set `select N` to auto-pick the top N.

topsongs {
    // Required for Last.fm
    username "your_lastfm_username" // your Last.fm account name
    api_key "your_lastfm_api_key"   // or set env LASTFM_API_KEY

    // Optional defaults
    period "overall"   // overall | 7day | 1month | 3month | 6month | 12month
    limit 10           // how many top tracks to fetch/display from Last.fm
    //select 3         // optional: auto-include top N; omit to choose interactively

    // Rendering
    format "  - {artist} - {track}" // tokens: {artist}, {track}, {playcount}
    join "\n"                     // string between rows
    //prefix "**On Loop**:\n"    // text before the list
    //suffix ""                 // text after the list

    // Title cleanup
    strip_feat true     // remove "feat." and similar from track titles
    strip_feat_regex "(?i)\\s*(?:[\\(\\[]\\s*(?:feat\\.?|ft\\.?|with)\\b.*?[\\)\\]]|-\\s*(?:feat\\.?|ft\\.?|with)\\b.*)$"

    // Convenience
    copy false          // copy final output to clipboard (Windows only)
    debug false         // verbose HTTP logging; shows request line/headers and error bodies

    // Discord (manual updates preferred; use --discord-dry-run/--update-discord if needed)
    // Provide your user token only if you intend to use Discord operations
    discord_token ""
    // Regex to find the section in your current bio to replace
    discord_bio_regex "/\\*\\*[\\w ]+\\*\\*:?[\r]?(\n[ \\w-]+)+\n/"
    //update_discord true       // perform actual PATCH to update the bio (requires token and templates)
    //discord_dry_run true      // preview the replacement only; no PATCH
}
"#;
