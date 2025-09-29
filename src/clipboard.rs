use anyhow::{anyhow, Result};

#[cfg(target_os = "windows")]
pub fn copy_to_clipboard(s: &str) -> Result<()> {
    use clipboard_win::formats::Unicode;
    clipboard_win::set_clipboard(Unicode, s)
        .map_err(|e| anyhow!("Windows clipboard error: {}", e))
}

#[cfg(not(target_os = "windows"))]
pub fn copy_to_clipboard(_s: &str) -> Result<()> {
    Err(anyhow!("Clipboard copy is only supported on Windows in this build. Omit --copy or run on Windows."))
}
