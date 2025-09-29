use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::RequestBuilder;
use std::collections::HashMap;

pub struct HttpSpec {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

pub fn parse_http_spec(content: &str) -> Result<HttpSpec> {
    // Strip an optional UTF-8 BOM at the start to avoid corrupting the HTTP method token
    let content_no_bom = content.strip_prefix('\u{feff}').unwrap_or(content);
    let normalized = content_no_bom.replace("\r\n", "\n");
    let mut lines = normalized.lines();
    // Skip initial empty/comment lines
    let mut first_line = None;
    while let Some(line) = lines.next() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        first_line = Some(l.to_string());
        break;
    }
    let first = first_line.ok_or_else(|| anyhow!(".http file missing request line"))?;
    let mut parts = first.split_whitespace();
    // Remove any lingering BOM on the method token as an extra safeguard
    let method = parts
        .next()
        .map(|m| m.trim_start_matches('\u{feff}').to_string())
        .ok_or_else(|| anyhow!("Missing HTTP method"))?;
    let url = parts.next().ok_or_else(|| anyhow!("Missing URL"))?.to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();
    let mut in_body = false;
    for line in lines {
        let raw = line;
        if !in_body {
            if raw.trim().is_empty() {
                in_body = true;
                continue;
            }
            if raw.trim_start().starts_with('#') {
                continue;
            }
            if let Some(idx) = raw.find(':') {
                let (name, value) = raw.split_at(idx);
                let value = value.trim_start_matches(':').trim().to_string();
                headers.push((name.trim().to_string(), value));
            } else {
                return Err(anyhow!(format!("Invalid header line: {}", raw)));
            }
        } else {
            body_lines.push(raw.to_string());
        }
    }
    let body = if body_lines.is_empty() { None } else { Some(body_lines.join("\n")) };
    Ok(HttpSpec { method, url, headers, body })
}

pub fn build_vars_map(base: &[(&str, String)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in base {
        map.insert((*k).to_string(), v.clone());
    }
    // Also expose all environment variables
    for (k, v) in std::env::vars() {
        map.entry(k).or_insert(v);
    }
    map
}

pub fn substitute_vars(input: &str, vars: &HashMap<String, String>) -> String {
    // Replace {{NAME}} with value if present
    let re = Regex::new(r"\{\{([A-Za-z0-9_]+)\}\}").expect("regex compiles");
    re.replace_all(input, |caps: &regex::Captures| {
        let key = &caps[1];
        vars.get(key).cloned().unwrap_or_else(|| caps[0].to_string())
    }).to_string()
}

pub fn apply_substitution(spec: HttpSpec, vars: &HashMap<String, String>) -> HttpSpec {
    let url = substitute_vars(&spec.url, vars);
    let headers = spec.headers
        .into_iter()
        .map(|(k, v)| (substitute_vars(&k, vars), substitute_vars(&v, vars)))
        .collect();
    let body = spec.body.map(|b| substitute_vars(&b, vars));
    HttpSpec { method: spec.method, url, headers, body }
}

pub fn build_request_from_spec(client: &reqwest::Client, spec: &HttpSpec) -> Result<(RequestBuilder, Option<String>)> {
    let method = reqwest::Method::from_bytes(spec.method.as_bytes())
        .map_err(|_| anyhow!(format!("Unsupported HTTP method: {}", spec.method)))?;
    let mut rb = client.request(method, &spec.url);
    for (k, v) in &spec.headers {
        rb = rb.header(k, v);
    }
    let mut body_preview = None;
    if let Some(body) = &spec.body {
        rb = rb.body(body.clone());
        body_preview = Some(body.clone());
    }
    Ok((rb, body_preview))
}
