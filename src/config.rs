//! Optional user settings from `config.toml` next to the jot files.
//!
//! ```toml
//! [[highlight]]
//! pattern = '\b[a-z]+-[0-9a-f]{6}\b'   # issue IDs like abc-1f2e3d
//! style = "bold yellow"                # optional, this is the default
//! ```

use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;

use crate::store;

const DEFAULT_STYLE: &str = "bold yellow";

#[derive(Default)]
pub struct Config {
    pub highlights: Vec<Highlight>,
}

pub struct Highlight {
    pattern: Regex,
    /// SGR parameters, e.g. `1;33` for bold yellow.
    sgr: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    highlight: Vec<HighlightEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HighlightEntry {
    pattern: String,
    style: Option<String>,
}

pub fn path() -> Result<PathBuf> {
    Ok(store::config_dir()?.join("config.toml"))
}

impl Config {
    /// Load `config.toml`; a missing file means defaults.
    pub fn load() -> Result<Config> {
        let path = path()?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        Config::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    fn parse(text: &str) -> Result<Config> {
        let file: File = toml::from_str(text)?;
        let highlights = file
            .highlight
            .into_iter()
            .map(|entry| {
                let pattern = Regex::new(&entry.pattern)
                    .with_context(|| format!("highlight pattern {:?}", entry.pattern))?;
                let sgr = sgr(entry.style.as_deref().unwrap_or(DEFAULT_STYLE))?;
                Ok(Highlight { pattern, sgr })
            })
            .collect::<Result<_>>()?;
        Ok(Config { highlights })
    }

    /// Wrap every highlight match in `text` in its style. `text` may already contain ANSI
    /// styling (rendered markdown); matching ignores it, and styles active at the end of a
    /// match are restored afterwards.
    pub fn highlight(&self, text: &str) -> String {
        if self.highlights.is_empty() {
            return text.to_string();
        }

        // The visible text, and where each of its bytes sits in `text`.
        let mut plain = String::with_capacity(text.len());
        let mut offsets = Vec::with_capacity(text.len() + 1);
        let mut rest = text;
        while let Some(c) = rest.chars().next() {
            let at = text.len() - rest.len();
            let len = escape_len(rest).unwrap_or(c.len_utf8());
            if len == c.len_utf8() && !rest.starts_with('\x1b') {
                plain.push(c);
                offsets.extend((0..len).map(|k| at + k));
            }
            rest = &rest[len..];
        }
        offsets.push(text.len());

        // Earlier patterns win where matches overlap.
        let mut matches: Vec<(usize, usize, &str)> = Vec::new();
        for h in &self.highlights {
            for m in h.pattern.find_iter(&plain).filter(|m| !m.is_empty()) {
                if matches
                    .iter()
                    .all(|&(s, e, _)| m.end() <= s || m.start() >= e)
                {
                    matches.push((m.start(), m.end(), &h.sgr));
                }
            }
        }
        if matches.is_empty() {
            return text.to_string();
        }
        matches.sort_by_key(|&(s, _, _)| s);

        let mut out = String::with_capacity(text.len() + matches.len() * 16);
        let mut copied = 0;
        for (start, end, sgr) in matches {
            let (start, end) = (offsets[start], offsets[end]);
            out.push_str(&text[copied..start]);
            out.push_str(&format!("\x1b[{sgr}m"));
            out.push_str(&text[start..end]);
            out.push_str("\x1b[0m");
            out.push_str(&active_styles(&text[..end]));
            copied = end;
        }
        out.push_str(&text[copied..]);
        out
    }
}

/// Length of the ANSI escape sequence at the start of `s`, if there is one.
fn escape_len(s: &str) -> Option<usize> {
    let body = s.strip_prefix("\x1b[")?;
    let end = body.bytes().position(|b| (0x40..=0x7e).contains(&b))?;
    Some(2 + end + 1)
}

/// The SGR sequences still in effect at the end of `text`, to re-apply after a reset.
fn active_styles(text: &str) -> String {
    let mut active = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("\x1b[") {
        rest = &rest[i..];
        let Some(len) = escape_len(rest) else { break };
        let seq = &rest[..len];
        if seq.ends_with('m') {
            if matches!(seq, "\x1b[m" | "\x1b[0m") {
                active.clear();
            } else {
                active.push_str(seq);
            }
        }
        rest = &rest[len..];
    }
    active
}

/// Translate a style like `bold yellow` or `underline #ff8800` into SGR parameters.
fn sgr(style: &str) -> Result<String> {
    let mut codes = Vec::new();
    for word in style.split_whitespace() {
        let code = match word.to_ascii_lowercase().as_str() {
            "bold" => "1".into(),
            "dim" => "2".into(),
            "italic" => "3".into(),
            "underline" => "4".into(),
            "reverse" => "7".into(),
            "black" => "30".into(),
            "red" => "31".into(),
            "green" => "32".into(),
            "yellow" => "33".into(),
            "blue" => "34".into(),
            "magenta" => "35".into(),
            "cyan" => "36".into(),
            "white" => "37".into(),
            "gray" | "grey" => "90".into(),
            hex if hex.len() == 7 && hex.starts_with('#') => {
                let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
                match (channel(1), channel(3), channel(5)) {
                    (Ok(r), Ok(g), Ok(b)) => format!("38;2;{r};{g};{b}"),
                    _ => bail!("not a color: {word}"),
                }
            }
            _ => bail!(
                "unknown style {word:?} (use bold, dim, italic, underline, reverse, \
                 a color name or #rrggbb)"
            ),
        };
        codes.push(code);
    }
    if codes.is_empty() {
        bail!("empty highlight style");
    }
    Ok(codes.join(";"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(toml: &str) -> Config {
        Config::parse(toml).unwrap()
    }

    #[test]
    fn highlights_plain_text() {
        let c = config("[[highlight]]\npattern = '\\b[a-z]+-[a-z0-9]{8}\\b'\n");
        assert_eq!(
            c.highlight("see web-1a2b3c4d now"),
            "see \x1b[1;33mweb-1a2b3c4d\x1b[0m now"
        );
        assert_eq!(c.highlight("no ids here"), "no ids here");
    }

    #[test]
    fn restores_surrounding_styles() {
        let c = config("[[highlight]]\npattern = 'ID'\nstyle = 'red'\n");
        // A match inside bold text gets bold back afterwards.
        assert_eq!(
            c.highlight("\x1b[1mmy ID here\x1b[0m"),
            "\x1b[1mmy \x1b[31mID\x1b[0m\x1b[1m here\x1b[0m"
        );
    }

    #[test]
    fn matches_across_escapes_and_first_pattern_wins() {
        let c = config(
            "[[highlight]]\npattern = 'ab'\nstyle = 'red'\n\
             [[highlight]]\npattern = 'bc'\nstyle = 'blue'\n",
        );
        assert_eq!(
            c.highlight("a\x1b[1mbc"),
            "\x1b[31ma\x1b[1mb\x1b[0m\x1b[1mc"
        );
    }

    #[test]
    fn parses_styles() {
        assert_eq!(sgr("bold yellow").unwrap(), "1;33");
        assert_eq!(sgr("underline #ff8800").unwrap(), "4;38;2;255;136;0");
        assert!(sgr("sparkly").is_err());
        assert!(sgr("").is_err());
    }

    #[test]
    fn rejects_bad_config() {
        assert!(Config::parse("[[highlight]]\npattern = '['\n").is_err());
        assert!(Config::parse("[[highlight]]\npatern = 'x'\n").is_err());
        assert!(config("").highlights.is_empty());
    }
}
