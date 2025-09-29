use crate::lastfm::Track;

pub fn render_template(tpl: &str, track: &Track) -> String {
    tpl.replace("{artist}", &track.artist.name)
        .replace("{track}", &track.name)
        .replace("{playcount}", &track.playcount)
}

// Interpret common backslash escape sequences so users can write \n, \t, etc. on the CLI.
pub fn interpret_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('0') => out.push('\0'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some(other) => {
                    // Unknown escape: keep the backslash and the char
                    out.push('\\');
                    out.push(other);
                }
                None => {
                    // Trailing backslash, keep it
                    out.push('\\');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
