use regex::Regex;

/// Remove wrapping slashes if present (e.g., "/abc/" -> "abc").
pub fn normalize_pattern(p: &str) -> String {
    let s = p.trim();
    if s.len() >= 2 && s.starts_with('/') && s.ends_with('/') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Strip featured-artist annotations from a track title using either a custom regex or a sensible default.
/// After stripping, leading/trailing whitespace and dashes are trimmed.
pub fn strip_title(title: &str, custom_regex: Option<&str>) -> String {
    // Default pattern removes things like:
    //  - "(feat. Artist)" or "[ft. Artist]" anywhere
    //  - trailing "- feat. Artist" or "- with Artist"
    // Case-insensitive, remove from the end when it matches those patterns.
    let default_pat = r"(?i)\s*(?:[\(\[]\s*(?:feat\.?|ft\.?|with)\b.*?[\)\]]|-\s*(?:feat\.?|ft\.?|with)\b.*)$";
    let pat = custom_regex
        .map(|r| normalize_pattern(r))
        .unwrap_or_else(|| default_pat.to_string());

    let re = Regex::new(&pat).unwrap_or_else(|_| Regex::new(default_pat).expect("default regex compiles"));
    let stripped = re.replace(title, "").to_string();
    // Trim common surrounding spaces and separators left behind
    let stripped = stripped.trim();
    // Also trim a trailing dash or colon if left at the end after removal
    let stripped = stripped.trim_end_matches(['-', ':', '–', '—', '|', '/']).trim();
    stripped.to_string()
}
