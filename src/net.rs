use anyhow::{anyhow, Result};

fn dim(s: &str) -> String {
    // ANSI dim; safe fallback if terminal doesn't support it
    format!("\x1b[2m{}\x1b[0m", s)
}

fn redact_header(name: &str, value: &str) -> String {
    let lname = name.to_ascii_lowercase();
    if lname == "authorization" || lname == "cookie" {
        return "<redacted>".to_string();
    }
    value.to_string()
}

fn redact_url(url: &str) -> String {
    // Very light redaction for common secrets in query
    let mut out = url.to_string();
    for key in ["api_key", "apikey", "token", "auth", "authorization"] {
        if let Some(idx) = out.find(&format!("{}=", key)) {
            // replace the value until next & or end
            let start = idx + key.len() + 1;
            let end = out[start..]
                .find('&')
                .map(|i| start + i)
                .unwrap_or_else(|| out.len());
            out.replace_range(start..end, "<redacted>");
        }
    }
    out
}

pub async fn send_with_debug(rb: reqwest::RequestBuilder, debug: bool, body_preview: Option<String>) -> Result<reqwest::Response> {
    if debug {
        if let Some(cloned) = rb.try_clone() {
            match cloned.build() {
                Ok(req) => {
                    let line = format!("{} {}", req.method(), redact_url(req.url().as_str()));
                    eprintln!("{}", dim(&format!("→ Request: {}", line)));
                    // headers
                    for (name, value) in req.headers().iter() {
                        let val = value.to_str().unwrap_or("<non-utf8>");
                        let red = redact_header(name.as_str(), val);
                        eprintln!("{}", dim(&format!("  {}: {}", name, red)));
                    }
                    if let Some(b) = &body_preview {
                        if !b.trim().is_empty() {
                            eprintln!("{}", dim("  (body):"));
                            for line in b.lines() {
                                eprintln!("{}", dim(&format!("    {}", line)));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}", dim(&format!("(failed to build request for debug: {})", e)));
                }
            }
        }
    }

    let resp_res = rb.send().await;
    if let Err(e) = &resp_res {
        if debug {
            eprintln!("HTTP request send error: {}", e);
        }
    }
    let resp = resp_res?;

    if debug {
        eprintln!("{}", dim(&format!("← Response: {}", resp.status())));
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("<failed to read error body: {}>", e));
        if debug {
            eprintln!("HTTP error status: {}\nResponse body: {}", status, body);
        }
        return Err(anyhow!(format!("HTTP request failed with status {}", status)));
    }
    Ok(resp)
}
