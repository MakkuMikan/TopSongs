use clap::{ArgGroup, Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Period {
    Overall,
    #[value(name = "7day")] SevenDay,
    #[value(name = "1month")] OneMonth,
    #[value(name = "3month")] ThreeMonth,
    #[value(name = "6month")] SixMonth,
    #[value(name = "12month")] TwelveMonth,
}

impl Period {
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Period::Overall => "overall",
            Period::SevenDay => "7day",
            Period::OneMonth => "1month",
            Period::ThreeMonth => "3month",
            Period::SixMonth => "6month",
            Period::TwelveMonth => "12month",
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "topsongs", version, about = "Fetch Last.fm top tracks and format them for your Discord bio", long_about = None)]
#[command(group(
    ArgGroup::new("auth")
        .args(["api_key"]) // keeping for future expansion
        .multiple(true)
))]
pub struct Cli { 
    /// Generate an example KDL config (topsongs.config.kdl) in the current directory and exit
    #[arg(short = 'G', long)]
    pub generate_config: bool, 

    /// Generate barebones .http templates: use without a value to create all missing defaults, or pass one of [lastfm_top_tracks | discord_get_me | discord_patch_bio] to create a specific file if missing; then exit
    #[arg(long = "generate-http", value_name = "TEMPLATE", num_args = 0..=1, default_missing_value = "ALL")]
    pub generate_http: Option<String>,

    /// Last.fm username (can be set via config file)
    #[arg(short, long)]
    pub username: Option<String>,

    /// Last.fm API key (or set LASTFM_API_KEY env var)
    #[arg(short = 'k', long)]
    pub api_key: Option<String>,

    /// Time period for top tracks (CLI overrides config when provided)
    #[arg(short, long, value_enum)]
    pub period: Option<Period>,

    /// Number of top tracks to fetch/display
    #[arg(short = 'n', long, default_value_t = 10, value_parser = clap::value_parser!(u32))]
    pub limit: u32,

    /// Show Last.fm results only and exit (skip the selection and rendering process)
    #[arg(short = 'Q', long = "query")]
    pub query: bool,

    /// Automatically include the top N tracks (skips interactive selection). If omitted, you'll be prompted to choose interactively.
    #[arg(short, long, value_parser = clap::value_parser!(usize))]
    pub select: Option<usize>,

    /// Format template for each entry. Tokens: {artist}, {track}, {playcount}
    #[arg(short = 'f', long, default_value = "  - {artist} - {track}")]
    pub format: String,

    /// Joiner between entries
    #[arg(short = 'j', long, default_value = "\n")]
    pub join: String,

    /// String to place before the joined entries (e.g. "**Song Recs**:\n")
    #[arg(long, default_value = "")]
    pub prefix: String,

    /// String to place after the joined entries
    #[arg(long, default_value = "")]
    pub suffix: String,

    /// If set, remove trailing featured-artist annotations like "(feat. ...)" or "- ft. ..." from track titles, then trim spaces
    #[arg(short = 't', long)]
    pub strip_feat: bool,

    /// Optional custom regex to strip from track titles (Rust regex). If surrounded by slashes, they will be stripped. Used only if --strip-feat is set
    #[arg(long)]
    pub strip_feat_regex: Option<String>,

    /// Copy the generated bio string to clipboard (Windows)
    #[arg(short = 'c', long)]
    pub copy: bool,

    /// Discord user token; used for Discord operations when enabled via --update-discord or --discord-dry-run (or set DISCORD_TOKEN env var)
    #[arg(long)]
    pub discord_token: Option<String>,

    /// Regex to locate the section of your bio to replace (use Rust regex syntax). If surrounded by slashes, they will be stripped.
    #[arg(long, default_value = r"/\*\*[\w ]+\*\*:?\r?(\n[ \w-]+)+\n/")]
    pub discord_bio_regex: String,

    /// Perform Discord operations (fetch/preview/update). If not set, no Discord calls will be made even if DISCORD_TOKEN is present.
    #[arg(short = 'U', long)]
    pub update_discord: bool,

    /// Dry-run Discord changes: fetch current bio and show the replacement result, but do not PATCH.
    #[arg(short = 'r', long)]
    pub discord_dry_run: bool,

    /// Enable verbose logging: prints HTTP request details and response statuses (and bodies on errors)
    #[arg(short = 'd', long)]
    pub debug: bool,

}
